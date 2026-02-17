#!/usr/bin/env python3
"""Fast batch scraper + merge with schema data to produce final cards.json.

Uses parallel RSC fetches (batches of 20) for speed, then merges
attack/ability/effect data with the pre-scraped schema metadata.
"""

import asyncio
import json
import os
import re
import sys
import time

from playwright.async_api import async_playwright

DATA_DIR = os.path.join(os.path.dirname(__file__), "..", "data")
BASE_URL = "https://pocket.pokemongohub.net"


def parse_attacks_from_rsc(rsc_text: str) -> list:
    """Extract attacks using the cost-{type}-{index} pattern in RSC payload."""
    attacks = []
    name_pattern = re.compile(
        r'"h3",null,\{"className":"text-lg font-bold","children":"([^"]+)"\}'
    )
    damage_pattern = re.compile(r'"font-bold","children":\[(\d+),')
    effect_pattern = re.compile(r'"text-sm pt-1","children":"([^"]*)"')
    cost_pattern = re.compile(r'"cost-(\w+)-(\d+)"')

    skip_names = {"From a Regular Pack", "From a Rare Pack", "Set", "From a Wonder Pick"}

    for name_match in name_pattern.finditer(rsc_text):
        attack_name = name_match.group(1).strip().replace("\t", "")
        if attack_name in skip_names:
            continue

        attack = {"name": attack_name, "energy_cost": [], "damage": 0, "effect": None}

        before = rsc_text[max(0, name_match.start() - 800) : name_match.start()]
        costs = cost_pattern.findall(before)
        attack["energy_cost"] = [c[0] for c in costs]

        after = rsc_text[name_match.end() : name_match.end() + 300]
        dm = damage_pattern.search(after)
        if dm:
            attack["damage"] = int(dm.group(1))

        em = effect_pattern.search(after)
        if em and em.group(1):
            attack["effect"] = em.group(1)

        attacks.append(attack)

    return attacks


def parse_ability_from_rsc(rsc_text: str) -> dict | None:
    """Extract ability from RSC payload."""
    m = re.search(
        r'"Ability".*?"text-lg font-bold","children":"([^"]+)".*?"text-sm[^"]*","children":"([^"]+)"',
        rsc_text[:30000],
        re.DOTALL,
    )
    if m:
        return {"name": m.group(1), "description": m.group(2)}
    return None


def parse_trainer_effect(rsc_text: str) -> str | None:
    match = re.search(r'"Description".*?"children":"([^"]{5,})"', rsc_text[:20000])
    return match.group(1) if match else None


def detect_card_type(rsc_text: str) -> str:
    """Detect card type from RSC payload."""
    check = rsc_text[:10000]
    if '"Supporter"' in check or '"supporter"' in check:
        return "supporter"
    if '"Item"' in check:
        return "item"
    if "Pokémon Tool" in check or '"Tool"' in check:
        return "tool"
    if '"Fossil"' in check:
        return "fossil"
    return "pokemon"


def parse_evolves_from(rsc_text: str) -> str | None:
    m = re.search(
        r"evolves from (\w[\w\s\-']*?)[\.\,]", rsc_text, re.IGNORECASE
    )
    return m.group(1).strip() if m else None


async def batch_fetch_rsc(page, urls: list[str]) -> list[tuple[str, str]]:
    """Fetch multiple RSC payloads in parallel using browser context."""
    results = await page.evaluate(
        """async (urls) => {
            const results = await Promise.allSettled(
                urls.map(async (url) => {
                    const resp = await fetch(url, {
                        headers: { 'RSC': '1', 'Next-Url': url }
                    });
                    return { url, text: await resp.text() };
                })
            );
            return results.map((r, i) => ({
                url: urls[i],
                text: r.status === 'fulfilled' ? r.value.text : null,
                error: r.status === 'rejected' ? r.reason?.message : null
            }));
        }""",
        urls,
    )
    return results


async def main():
    urls_path = os.path.join(DATA_DIR, "card_urls.json")
    schema_path = os.path.join(DATA_DIR, "cards_schema.json")
    output_path = os.path.join(DATA_DIR, "cards_final.json")

    # Load prerequisites
    with open(urls_path) as f:
        card_urls = json.load(f)
    with open(schema_path) as f:
        schema_data = json.load(f)

    print(f"URLs: {len(card_urls)}, Schema entries: {len(schema_data)}", flush=True)

    async with async_playwright() as p:
        browser = await p.chromium.launch(
            headless=True,
            args=["--disable-blink-features=AutomationControlled"],
        )
        context = await browser.new_context(
            user_agent="Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
        )
        page = await context.new_page()

        # Pass Cloudflare
        print("Passing Cloudflare...", flush=True)
        await page.goto(f"{BASE_URL}/en", timeout=60000)
        for _ in range(15):
            await page.wait_for_timeout(2000)
            if "Just a moment" not in await page.title():
                break
        title = await page.title()
        print(f"OK: {title}", flush=True)

        # Batch fetch RSC payloads
        BATCH_SIZE = 20
        all_rsc = {}  # slug -> rsc_text
        failed = []
        start = time.time()

        for batch_start in range(0, len(card_urls), BATCH_SIZE):
            batch = card_urls[batch_start : batch_start + BATCH_SIZE]
            try:
                results = await batch_fetch_rsc(page, batch)
                for r in results:
                    slug = r["url"].split("/")[-1]
                    if r["text"]:
                        all_rsc[slug] = (r["url"], r["text"])
                    else:
                        failed.append(r["url"])
            except Exception as e:
                print(f"  Batch error at {batch_start}: {e}", flush=True)
                # Fall back to sequential for this batch
                for url in batch:
                    try:
                        rsc = await page.evaluate(
                            """async (url) => {
                                const resp = await fetch(url, {
                                    headers: { 'RSC': '1', 'Next-Url': url }
                                });
                                return await resp.text();
                            }""",
                            url,
                        )
                        slug = url.split("/")[-1]
                        all_rsc[slug] = (url, rsc)
                    except Exception as e2:
                        failed.append(url)

            done = batch_start + len(batch)
            if done % 200 == 0 or done == len(card_urls):
                elapsed = time.time() - start
                rate = done / elapsed if elapsed > 0 else 0
                remaining = (len(card_urls) - done) / rate if rate > 0 else 0
                print(
                    f"  [{done}/{len(card_urls)}] {elapsed:.0f}s elapsed, "
                    f"~{remaining:.0f}s remaining, {len(failed)} failed",
                    flush=True,
                )

        await browser.close()

    elapsed = time.time() - start
    print(f"\nFetched {len(all_rsc)} RSC payloads in {elapsed:.0f}s ({len(failed)} failed)", flush=True)

    # Parse and merge
    cards = []
    seen_slugs = set()

    for slug, (url, rsc_text) in all_rsc.items():
        if slug in seen_slugs:
            continue
        seen_slugs.add(slug)

        # Get schema metadata
        schema = schema_data.get(slug, {})
        name_raw = schema.get("name", "")
        # Clean name: remove "(#N, Set Name)" suffix
        name_match = re.match(r"(.+?)\s*\(#\d+", name_raw)
        name = name_match.group(1).strip() if name_match else name_raw

        card_type = detect_card_type(rsc_text)

        # If schema says no HP but we classified as pokemon, it's a trainer
        if card_type == "pokemon" and schema.get("hp") is None:
            card_type = "item"  # default trainer type for unclassified

        card = {
            "slug": slug,
            "url": url,
            "name": name,
            "card_type": card_type,
            "hp": schema.get("hp"),
            "stage": schema.get("stage"),
            "energy_type": schema.get("energy_type"),
            "weakness": schema.get("weakness"),
            "retreat_cost": schema.get("retreat_cost"),
            "set_name": None,
            "card_number": None,
            "rarity": schema.get("rarity"),
        }

        # Extract set_name and card_number from schema name
        set_match = re.search(r"\(#(\d+),\s*(.+?)\)", name_raw)
        if set_match:
            card["card_number"] = int(set_match.group(1))
            card["set_name"] = set_match.group(2).strip()

        if card_type == "pokemon":
            card["attacks"] = parse_attacks_from_rsc(rsc_text)
            card["ability"] = parse_ability_from_rsc(rsc_text)
            card["is_ex"] = " ex" in name.lower() or " EX" in name
            card["evolves_from"] = parse_evolves_from(rsc_text)
        else:
            card["attacks"] = []
            card["ability"] = None
            card["is_ex"] = False
            card["evolves_from"] = None
            card["effect"] = parse_trainer_effect(rsc_text)

        cards.append(card)

    # Deduplicate: keep unique by (name, set_name, card_number)
    # Some cards appear multiple times (different art variants)
    unique_cards = []
    seen_key = set()
    for c in cards:
        key = (c["name"], c.get("set_name"), c.get("card_number"))
        if key not in seen_key:
            seen_key.add(key)
            unique_cards.append(c)

    # Sort by set_name then card_number
    unique_cards.sort(key=lambda c: (c.get("set_name") or "", c.get("card_number") or 0))

    # Save
    with open(output_path, "w") as f:
        json.dump(unique_cards, f, indent=2)

    # Stats
    pokemon = [c for c in unique_cards if c["card_type"] == "pokemon"]
    with_attacks = [c for c in pokemon if c.get("attacks")]
    with_hp = [c for c in pokemon if c.get("hp") and c["hp"] > 0]
    trainers = [c for c in unique_cards if c["card_type"] != "pokemon"]

    print(f"\nFinal: {len(unique_cards)} unique cards")
    print(f"  Pokemon: {len(pokemon)} ({len(with_attacks)} with attacks, {len(with_hp)} with HP)")
    print(f"  Trainers: {len(trainers)}")
    print(f"  Saved to {output_path}", flush=True)


if __name__ == "__main__":
    asyncio.run(main())
