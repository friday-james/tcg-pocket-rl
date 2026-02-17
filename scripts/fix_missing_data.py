#!/usr/bin/env python3
"""Fix missing card data by re-fetching RSC payloads and improving parsing.

Targets:
- Pokemon with missing HP (Eevee Grove set)
- Trainer cards missing effect text
- Pokemon missing abilities
- Pokemon missing attacks
- Pokemon missing evolves_from
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


def parse_ability_v2(rsc_text: str) -> dict | None:
    """Improved ability extraction from RSC payload."""
    # Pattern 1: Look for "Ability" label followed by ability name and description
    # The RSC format typically has: "Ability" ... "text-lg font-bold" ... ability_name ... description
    patterns = [
        # Pattern: ability section with name and description
        re.compile(
            r'"Ability".*?"text-lg font-bold","children":"([^"]+)".*?"text-sm[^"]*","children":"([^"]+)"',
            re.DOTALL,
        ),
        # Pattern: ability with different class structure
        re.compile(
            r'"Ability".*?"font-bold[^"]*","children":"([^"]+)".*?"children":"([^"]{10,})"',
            re.DOTALL,
        ),
        # Pattern: look for ability indicator and extract name
        re.compile(
            r'ability.*?"children":"([^"]+)".*?"children":"([^"]{10,})"',
            re.DOTALL | re.IGNORECASE,
        ),
    ]

    for pat in patterns:
        m = pat.search(rsc_text[:40000])
        if m:
            name = m.group(1).strip()
            desc = m.group(2).strip()
            # Filter out false positives
            skip = {"From a Regular Pack", "From a Rare Pack", "Set", "From a Wonder Pick",
                    "Released on", "Cost to craft", "Cost to craft with"}
            if any(s in name for s in skip) or any(s in desc for s in skip):
                continue
            return {"name": name, "description": desc}
    return None


def parse_trainer_effect_v2(rsc_text: str) -> str | None:
    """Improved trainer effect extraction."""
    # Try multiple patterns
    patterns = [
        # Pattern 1: Effect in card description section
        re.compile(r'"card-effect[^"]*".*?"children":"([^"]{10,})"', re.DOTALL),
        # Pattern 2: Description children
        re.compile(r'"Description".*?"children":"([^"]{10,})"', re.DOTALL),
        # Pattern 3: text-sm with substantial content
        re.compile(r'"text-sm[^"]*","children":"([^"]{20,})"'),
        # Pattern 4: Any substantial text after card type indicators
        re.compile(r'(?:Supporter|Item|Tool).*?"children":"([^"]{20,})"', re.DOTALL),
    ]

    skip_phrases = {"Cost to craft", "Released on", "From a Regular Pack",
                    "From a Rare Pack", "From a Wonder Pick", "Set"}

    for pat in patterns:
        for m in pat.finditer(rsc_text[:30000]):
            text = m.group(1).strip()
            if any(s in text for s in skip_phrases):
                continue
            if len(text) > 10:
                return text
    return None


def parse_hp_from_rsc(rsc_text: str) -> int | None:
    """Extract HP directly from RSC payload."""
    # Look for HP value in schema.org
    m = re.search(r'"HP"[^}]*?"value"\s*:\s*"?(\d+)"?', rsc_text[:10000])
    if m:
        return int(m.group(1))
    # Look for HP in other formats
    m = re.search(r'(\d+)\s*HP', rsc_text[:5000])
    if m:
        return int(m.group(1))
    return None


def parse_evolves_from_rsc(rsc_text: str) -> str | None:
    """Extract evolves_from from RSC payload."""
    patterns = [
        re.compile(r"[Ee]volves from (\w[\w\s\-':.]*?)[\.\,\"]"),
        re.compile(r'"evolvesFrom"[^"]*"([^"]+)"'),
        re.compile(r'Evolves from[^"]*"children":"([^"]+)"'),
    ]
    for pat in patterns:
        m = pat.search(rsc_text[:20000])
        if m:
            name = m.group(1).strip()
            if name and len(name) < 30:
                return name
    return None


async def batch_fetch_rsc(page, urls: list[str]) -> list:
    """Fetch multiple RSC payloads in parallel."""
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
    cards_path = os.path.join(DATA_DIR, "cards.json")
    with open(cards_path) as f:
        cards = json.load(f)

    # Build index by slug
    card_index = {c["slug"]: c for c in cards}

    # Identify cards that need fixing
    need_hp = [c for c in cards if c["card_type"] == "pokemon" and (not c.get("hp") or c["hp"] == 0)]
    need_effect = [c for c in cards if c["card_type"] != "pokemon" and not c.get("effect")]
    pokemon = [c for c in cards if c["card_type"] == "pokemon"]
    need_ability_check = pokemon  # Check all pokemon for abilities
    need_attacks = [c for c in pokemon if not c.get("attacks") or len(c["attacks"]) == 0]
    need_evolves = [c for c in pokemon if not c.get("evolves_from") and c.get("stage") in ("stage 1", "stage 2")]

    # Collect all unique URLs that need re-fetching
    urls_to_fetch = set()
    for c in need_hp + need_effect + need_attacks + need_evolves:
        urls_to_fetch.add(c["url"])
    # For abilities, check all pokemon (since we have 0 abilities detected)
    for c in pokemon:
        urls_to_fetch.add(c["url"])

    urls_list = sorted(urls_to_fetch)
    print(f"Cards needing fixes:")
    print(f"  Missing HP: {len(need_hp)}")
    print(f"  Missing effect: {len(need_effect)}")
    print(f"  Missing attacks: {len(need_attacks)}")
    print(f"  Missing evolves_from: {len(need_evolves)}")
    print(f"  Checking for abilities: {len(pokemon)}")
    print(f"  Total URLs to fetch: {len(urls_list)}")

    async with async_playwright() as p:
        browser = await p.chromium.launch(
            headless=True,
            args=["--disable-blink-features=AutomationControlled"],
        )
        context = await browser.new_context(
            user_agent="Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
        )
        page = await context.new_page()

        print("Passing Cloudflare...", flush=True)
        await page.goto(f"{BASE_URL}/en", timeout=60000)
        for _ in range(15):
            await page.wait_for_timeout(2000)
            if "Just a moment" not in await page.title():
                break
        print(f"OK: {await page.title()}", flush=True)

        # Batch fetch
        BATCH_SIZE = 20
        rsc_data = {}  # slug -> rsc_text
        start = time.time()

        for batch_start in range(0, len(urls_list), BATCH_SIZE):
            batch = urls_list[batch_start : batch_start + BATCH_SIZE]
            try:
                results = await batch_fetch_rsc(page, batch)
                for r in results:
                    slug = r["url"].split("/")[-1]
                    if r["text"]:
                        rsc_data[slug] = r["text"]
            except Exception as e:
                print(f"  Batch error at {batch_start}: {e}", flush=True)
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
                        rsc_data[slug] = rsc
                    except:
                        pass

            done = min(batch_start + BATCH_SIZE, len(urls_list))
            if done % 200 == 0 or done == len(urls_list):
                elapsed = time.time() - start
                print(f"  [{done}/{len(urls_list)}] {elapsed:.0f}s", flush=True)

        await browser.close()

    elapsed = time.time() - start
    print(f"\nFetched {len(rsc_data)} RSC payloads in {elapsed:.0f}s", flush=True)

    # Now fix the cards
    hp_fixed = 0
    effect_fixed = 0
    ability_fixed = 0
    attacks_fixed = 0
    evolves_fixed = 0

    # Save a few sample RSC texts for debugging
    debug_samples = {}

    for card in cards:
        slug = card["slug"]
        if slug not in rsc_data:
            continue

        rsc = rsc_data[slug]

        # Fix HP
        if card["card_type"] == "pokemon" and (not card.get("hp") or card["hp"] == 0):
            hp = parse_hp_from_rsc(rsc)
            if hp and hp > 0:
                card["hp"] = hp
                hp_fixed += 1

        # Fix trainer effects
        if card["card_type"] != "pokemon" and not card.get("effect"):
            effect = parse_trainer_effect_v2(rsc)
            if effect:
                card["effect"] = effect
                effect_fixed += 1
            else:
                debug_samples[f"trainer-{slug}"] = rsc[:3000]

        # Fix abilities
        if card["card_type"] == "pokemon":
            ability = parse_ability_v2(rsc)
            if ability:
                card["ability"] = ability
                ability_fixed += 1

        # Fix evolves_from
        if card["card_type"] == "pokemon" and not card.get("evolves_from") and card.get("stage") in ("stage 1", "stage 2"):
            evolves = parse_evolves_from_rsc(rsc)
            if evolves:
                card["evolves_from"] = evolves
                evolves_fixed += 1

    # Save fixed cards
    with open(cards_path, "w") as f:
        json.dump(cards, f, indent=2)

    # Save debug samples
    debug_path = os.path.join(DATA_DIR, "rsc_debug.json")
    with open(debug_path, "w") as f:
        json.dump(debug_samples, f, indent=2)

    print(f"\nFixes applied:")
    print(f"  HP: {hp_fixed}/{len(need_hp)} fixed")
    print(f"  Effects: {effect_fixed}/{len(need_effect)} fixed")
    print(f"  Abilities: {ability_fixed} found")
    print(f"  Evolves: {evolves_fixed}/{len(need_evolves)} fixed")
    print(f"  Saved to {cards_path}")

    # Final stats
    pokemon = [c for c in cards if c["card_type"] == "pokemon"]
    still_no_hp = sum(1 for c in pokemon if not c.get("hp") or c["hp"] == 0)
    still_no_attacks = sum(1 for c in pokemon if not c.get("attacks") or len(c["attacks"]) == 0)
    has_ability = sum(1 for c in pokemon if c.get("ability"))
    trainers = [c for c in cards if c["card_type"] != "pokemon"]
    still_no_effect = sum(1 for c in trainers if not c.get("effect"))

    print(f"\nRemaining gaps:")
    print(f"  Pokemon without HP: {still_no_hp}")
    print(f"  Pokemon without attacks: {still_no_attacks}")
    print(f"  Pokemon with abilities: {has_ability}")
    print(f"  Trainers without effects: {still_no_effect}")


if __name__ == "__main__":
    asyncio.run(main())
