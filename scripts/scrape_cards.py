#!/usr/bin/env python3
"""Scrape Pokemon TCG Pocket card data from pocket.pokemongohub.net.

Card-by-card with visible progress. Uses rendered pages for set listing,
then RSC endpoint for individual cards (fast).
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


def parse_card_from_rsc(rsc_text: str, card_url: str) -> dict:
    """Parse card data from Next.js RSC flight payload."""
    card = {"url": card_url, "slug": card_url.split("/")[-1]}

    # --- Extract schema.org JSON-LD Product data ---
    # The JSON has nested objects, so we can't use .*? - find start and parse with brace counting
    schema_start = rsc_text.find('{"@context":"https://schema.org","@type":["Product","CreativeWork"]')
    schema = None
    if schema_start >= 0:
        # Find the matching closing brace
        depth = 0
        for i in range(schema_start, min(schema_start + 5000, len(rsc_text))):
            if rsc_text[i] == '{':
                depth += 1
            elif rsc_text[i] == '}':
                depth -= 1
                if depth == 0:
                    try:
                        schema = json.loads(rsc_text[schema_start:i+1])
                    except json.JSONDecodeError:
                        pass
                    break
    if schema:
        try:
            name = schema.get("name", "")
            name_match = re.match(r"(.+?)\s*\(", name)
            card["name"] = name_match.group(1).strip() if name_match else name

            for prop in schema.get("additionalProperty", []):
                pname = prop.get("name", "")
                pval = prop.get("value", "")
                if pname == "HP":
                    card["hp"] = int(pval) if pval else 0
                elif pname == "Energy Type":
                    card["energy_type"] = pval.lower() if pval else None
                elif pname == "Retreat Cost":
                    card["retreat_cost"] = int(pval) if pval else 0
                elif pname == "Weakness":
                    parts = pval.split()
                    card["weakness"] = parts[0].lower() if parts else None
                elif pname == "Stage":
                    card["stage"] = pval.lower()
                elif pname == "Set":
                    card["set_name"] = pval
                elif pname == "Card Number":
                    card["card_number"] = int(pval) if pval else 0
                elif pname == "Rarity":
                    card["rarity"] = pval
        except (json.JSONDecodeError, ValueError):
            pass

    # --- Determine card type ---
    card_type = "pokemon"
    if '"Supporter"' in rsc_text[:5000] or '"supporter"' in rsc_text[:5000]:
        card_type = "supporter"
    elif '"Item"' in rsc_text[:10000]:
        card_type = "item"
    elif "Pokémon Tool" in rsc_text[:10000] or '"Tool"' in rsc_text[:10000]:
        card_type = "tool"
    elif '"Fossil"' in rsc_text[:10000]:
        card_type = "fossil"
    card["card_type"] = card_type

    # --- Extract attacks (Pokemon only) ---
    if card_type == "pokemon":
        card["attacks"] = parse_attacks_from_rsc(rsc_text)
        card["ability"] = parse_ability_from_rsc(rsc_text)
        card["is_ex"] = " ex" in card.get("name", "")
        evolves_match = re.search(
            r"evolves from (\w[\w\s\-']*?)[\.\,]", rsc_text, re.IGNORECASE
        )
        card["evolves_from"] = evolves_match.group(1).strip() if evolves_match else None
    else:
        card["effect"] = parse_trainer_effect(rsc_text)

    return card


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
        attack_name = name_match.group(1)
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


async def scrape_all_cards():
    os.makedirs(DATA_DIR, exist_ok=True)

    # Check for existing progress
    urls_path = os.path.join(DATA_DIR, "card_urls.json")
    progress_path = os.path.join(DATA_DIR, "cards_scraped.json")

    existing_cards = []
    existing_slugs = set()
    if os.path.exists(progress_path):
        with open(progress_path) as f:
            existing_cards = json.load(f)
            existing_slugs = {c["slug"] for c in existing_cards}
        print(f"Resuming: {len(existing_cards)} cards already scraped", flush=True)

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
        print(f"OK: {await page.title()}", flush=True)

        # Get card URLs (from cache or fresh)
        unique_urls = []
        if os.path.exists(urls_path):
            with open(urls_path) as f:
                unique_urls = json.load(f)
            print(f"Loaded {len(unique_urls)} card URLs from cache", flush=True)
        else:
            set_links = await page.eval_on_selector_all(
                'a[href*="/en/set/"]',
                'els => [...new Set(els.map(e => e.getAttribute("href")))]',
            )
            print(f"Found {len(set_links)} sets", flush=True)

            all_card_urls = []
            for set_url in set_links:
                set_name = set_url.split("-", 1)[-1] if "-" in set_url else set_url
                print(f"  Set: {set_name}...", end=" ", flush=True)
                await page.goto(f"{BASE_URL}{set_url}", timeout=60000)
                await page.wait_for_timeout(2000)
                card_links = await page.eval_on_selector_all(
                    'a[href*="/en/card/"]',
                    'els => [...new Set(els.map(e => e.getAttribute("href")))]',
                )
                print(f"{len(card_links)} cards", flush=True)
                all_card_urls.extend(card_links)

            seen = set()
            for url in all_card_urls:
                if url not in seen:
                    seen.add(url)
                    unique_urls.append(url)

            with open(urls_path, "w") as f:
                json.dump(unique_urls, f, indent=2)
            print(f"Saved {len(unique_urls)} unique card URLs", flush=True)

        # Filter already scraped
        to_scrape = [u for u in unique_urls if u.split("/")[-1] not in existing_slugs]
        print(f"\nCards to scrape: {len(to_scrape)} (skipping {len(existing_slugs)})", flush=True)

        all_cards = list(existing_cards)
        failed = []
        start_time = time.time()

        for i, card_url in enumerate(to_scrape):
            slug = card_url.split("/")[-1]
            try:
                rsc = await page.evaluate(
                    """async (url) => {
                        const resp = await fetch(url, {
                            headers: { 'RSC': '1', 'Next-Url': url }
                        });
                        return await resp.text();
                    }""",
                    card_url,
                )
                card = parse_card_from_rsc(rsc, card_url)
                all_cards.append(card)

                # Print progress for every card
                attacks_str = ""
                if card.get("attacks"):
                    attacks_str = " | " + ", ".join(
                        f"{a['name']}({''.join(e[0] for e in a['energy_cost'])}={a['damage']})"
                        for a in card["attacks"]
                    )
                print(
                    f"  [{i+1}/{len(to_scrape)}] {card.get('name', '?')} "
                    f"({card.get('card_type', '?')}, {card.get('hp', '-')}HP)"
                    f"{attacks_str}",
                    flush=True,
                )

            except Exception as e:
                print(f"  [{i+1}/{len(to_scrape)}] FAILED {slug}: {e}", flush=True)
                failed.append(card_url)

            # Save progress every 50 cards
            if (i + 1) % 50 == 0:
                with open(progress_path, "w") as f:
                    json.dump(all_cards, f, indent=2)
                elapsed = time.time() - start_time
                rate = (i + 1) / elapsed
                remaining = (len(to_scrape) - i - 1) / rate if rate > 0 else 0
                print(
                    f"  --- Saved progress: {len(all_cards)} total, "
                    f"{elapsed:.0f}s elapsed, ~{remaining:.0f}s remaining ---",
                    flush=True,
                )

        # Final save
        with open(progress_path, "w") as f:
            json.dump(all_cards, f, indent=2)

        elapsed = time.time() - start_time
        print(f"\nDone! {len(all_cards)} cards in {elapsed:.1f}s", flush=True)

        if failed:
            print(f"Failed: {len(failed)}", flush=True)
            with open(os.path.join(DATA_DIR, "cards_failed.json"), "w") as f:
                json.dump(failed, f, indent=2)

        # Stats
        pokemon = sum(1 for c in all_cards if c.get("card_type") == "pokemon")
        with_attacks = sum(1 for c in all_cards if c.get("attacks") and len(c["attacks"]) > 0)
        trainers = sum(1 for c in all_cards if c.get("card_type") in ("supporter", "item", "tool"))
        print(f"\nPokemon: {pokemon} ({with_attacks} with attacks)")
        print(f"Trainers: {trainers}")
        print(f"Total: {len(all_cards)}", flush=True)

        await browser.close()


if __name__ == "__main__":
    asyncio.run(scrape_all_cards())
