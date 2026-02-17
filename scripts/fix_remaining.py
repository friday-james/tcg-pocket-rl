#!/usr/bin/env python3
"""Fix remaining card data gaps:
- 21 Eevee Grove cards missing HP and attacks
- 5 cards with abilities but no attacks (attack parser confused by ability section)
- 52 Stage 1/2 missing evolves_from
"""

import asyncio
import json
import os
import re
import time

from playwright.async_api import async_playwright

DATA_DIR = os.path.join(os.path.dirname(__file__), "..", "data")
BASE_URL = "https://pocket.pokemongohub.net"


def parse_attacks_improved(rsc_text: str) -> list:
    """Improved attack parser that handles cards with abilities.

    Abilities use "text-lg text-red-700 font-bold" class.
    Attacks use "text-lg font-bold" WITHOUT red-700.
    We need to skip ability name matches.
    """
    attacks = []

    # Find ALL potential attack name positions
    # Standard pattern: "h3",null,{"className":"text-lg font-bold","children":"NAME"}
    name_pattern = re.compile(
        r'"h3",null,\{"className":"text-lg font-bold","children":"([^"]+)"\}'
    )
    # Also match variant with extra classes but NOT red-700
    name_pattern2 = re.compile(
        r'"h3",null,\{"className":"text-lg\s+(?!text-red)[^"]*font-bold","children":"([^"]+)"\}'
    )

    cost_pattern = re.compile(r'"cost-(\w+)-(\d+)"')
    damage_pattern = re.compile(r'"font-bold","children":\[(\d+),')
    effect_pattern = re.compile(r'"text-sm pt-1","children":"([^"]*)"')

    skip_names = {
        "From a Regular Pack", "From a Rare Pack", "Set",
        "From a Wonder Pick", "Ability", "Home",
    }

    # Collect all name positions from both patterns
    matches = list(name_pattern.finditer(rsc_text))
    for m2 in name_pattern2.finditer(rsc_text):
        # Add if not already found at same position
        if not any(m.start() == m2.start() for m in matches):
            matches.append(m2)
    matches.sort(key=lambda m: m.start())

    # Filter out ability names - they appear right after "Ability" label
    ability_region = None
    ability_match = re.search(
        r'"children":"Ability"\}.*?"text-lg\s+text-red-700\s+font-bold","children":"([^"]+)"',
        rsc_text[:40000],
        re.DOTALL,
    )
    if ability_match:
        # Mark the ability name region to skip
        ability_region = (ability_match.start(1) - 100, ability_match.end(1) + 100)

    for name_match in matches:
        attack_name = name_match.group(1).strip().replace("\t", "")
        if attack_name in skip_names:
            continue

        # Skip if this is inside the ability region
        if ability_region and ability_region[0] <= name_match.start() <= ability_region[1]:
            continue

        attack = {"name": attack_name, "energy_cost": [], "damage": 0, "effect": None}

        # Look backward for energy costs
        before = rsc_text[max(0, name_match.start() - 800): name_match.start()]
        costs = cost_pattern.findall(before)
        attack["energy_cost"] = [c[0] for c in costs]

        # Look forward for damage and effect
        after = rsc_text[name_match.end(): name_match.end() + 300]
        dm = damage_pattern.search(after)
        if dm:
            attack["damage"] = int(dm.group(1))

        em = effect_pattern.search(after)
        if em and em.group(1):
            attack["effect"] = em.group(1)

        attacks.append(attack)

    return attacks


def parse_hp_from_rsc(rsc_text: str) -> int | None:
    """Extract HP from RSC payload - multiple patterns."""
    # Pattern 1: schema.org additionalProperty
    schema_start = rsc_text.find('{"@context":"https://schema.org"')
    if schema_start >= 0:
        depth = 0
        for i in range(schema_start, min(schema_start + 5000, len(rsc_text))):
            if rsc_text[i] == '{':
                depth += 1
            elif rsc_text[i] == '}':
                depth -= 1
                if depth == 0:
                    try:
                        schema = json.loads(rsc_text[schema_start:i + 1])
                        for prop in schema.get("additionalProperty", []):
                            if prop.get("name") == "HP":
                                val = prop.get("value", "")
                                if val:
                                    return int(val)
                    except (json.JSONDecodeError, ValueError):
                        pass
                    break

    # Pattern 2: HP display in RSC - "HP","children":"120"
    m = re.search(r'"HP"[^}]*"children":"(\d+)"', rsc_text[:10000])
    if m:
        return int(m.group(1))

    # Pattern 3: HP badge - look for HP number near energy type
    m = re.search(r'"hp"\s*:\s*(\d+)', rsc_text[:10000])
    if m:
        return int(m.group(1))

    # Pattern 4: Direct HP text pattern
    m = re.search(r'HP\s*(\d+)|(\d+)\s*HP', rsc_text[:5000])
    if m:
        val = m.group(1) or m.group(2)
        hp = int(val)
        if 30 <= hp <= 350:
            return hp

    return None


def parse_evolves_from_rsc(rsc_text: str) -> str | None:
    """Extract evolves_from from RSC payload."""
    # Pattern: "Evolves from" text near a Pokemon name
    m = re.search(r'"Evolves from ([^"]+)"', rsc_text[:10000])
    if m:
        return m.group(1).strip()

    m = re.search(r'Evolves from\s+([A-Z][a-z]+(?:\s+[A-Za-z]+)*)', rsc_text[:10000])
    if m:
        return m.group(1).strip()

    return None


async def batch_fetch_rsc(page, urls: list[str]) -> list:
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

    # Identify cards that need fixing
    needs_fix = []
    for c in cards:
        if c["card_type"] != "pokemon":
            continue
        needs_hp = not c.get("hp") or c["hp"] == 0
        needs_attacks = not c.get("attacks") or len(c["attacks"]) == 0
        needs_evolves = c.get("stage") in ("stage 1", "stage 2") and not c.get("evolves_from")
        if needs_hp or needs_attacks or needs_evolves:
            needs_fix.append(c)

    urls_to_fetch = sorted(set(c["url"] for c in needs_fix))
    print(f"Cards needing fixes: {len(needs_fix)}")
    print(f"URLs to fetch: {len(urls_to_fetch)}")

    if not urls_to_fetch:
        print("Nothing to fix!")
        return

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

        # Fetch RSC payloads
        rsc_data = {}
        BATCH_SIZE = 10
        start = time.time()

        for batch_start in range(0, len(urls_to_fetch), BATCH_SIZE):
            batch = urls_to_fetch[batch_start: batch_start + BATCH_SIZE]
            try:
                results = await batch_fetch_rsc(page, batch)
                for r in results:
                    slug = r["url"].split("/")[-1]
                    if r["text"]:
                        rsc_data[slug] = r["text"]
            except Exception as e:
                print(f"  Batch error: {e}", flush=True)
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

        elapsed = time.time() - start
        print(f"Fetched {len(rsc_data)} RSC payloads in {elapsed:.0f}s", flush=True)

        # Debug: dump one RSC for analysis
        debug_slugs = ["22l26jtm1t096nq-alolan-persian", "duria2jmbx04s77-eevee"]
        for slug in debug_slugs:
            if slug in rsc_data:
                debug_path = os.path.join(DATA_DIR, f"debug_rsc_{slug[:20]}.txt")
                with open(debug_path, "w") as f:
                    f.write(rsc_data[slug])
                print(f"Debug RSC saved: {debug_path}")

        await browser.close()

    # Apply fixes
    hp_fixed = 0
    attacks_fixed = 0
    evolves_fixed = 0

    for card in needs_fix:
        slug = card["slug"]
        if slug not in rsc_data:
            continue
        rsc = rsc_data[slug]

        # Fix HP
        if not card.get("hp") or card["hp"] == 0:
            hp = parse_hp_from_rsc(rsc)
            if hp:
                card["hp"] = hp
                hp_fixed += 1

        # Fix attacks
        if not card.get("attacks") or len(card["attacks"]) == 0:
            attacks = parse_attacks_improved(rsc)
            if attacks:
                card["attacks"] = attacks
                attacks_fixed += 1

        # Fix evolves_from
        if card.get("stage") in ("stage 1", "stage 2") and not card.get("evolves_from"):
            evolves = parse_evolves_from_rsc(rsc)
            if evolves:
                card["evolves_from"] = evolves
                evolves_fixed += 1

    # Save
    with open(cards_path, "w") as f:
        json.dump(cards, f, indent=2)

    print(f"\nFixes applied:")
    print(f"  HP: {hp_fixed}")
    print(f"  Attacks: {attacks_fixed}")
    print(f"  Evolves from: {evolves_fixed}")

    # Final stats
    pokemon = [c for c in cards if c["card_type"] == "pokemon"]
    no_hp = sum(1 for c in pokemon if not c.get("hp") or c["hp"] == 0)
    no_attacks = sum(1 for c in pokemon if not c.get("attacks") or len(c["attacks"]) == 0)
    no_evolves = sum(
        1 for c in pokemon
        if c.get("stage") in ("stage 1", "stage 2") and not c.get("evolves_from")
    )
    print(f"\nRemaining gaps:")
    print(f"  Pokemon without HP: {no_hp}")
    print(f"  Pokemon without attacks: {no_attacks}")
    print(f"  Stage 1/2 without evolves_from: {no_evolves}")

    # Show what was fixed
    if attacks_fixed > 0:
        print(f"\nAttack fix examples:")
        for c in needs_fix:
            if c.get("attacks") and len(c["attacks"]) > 0:
                for a in c["attacks"]:
                    print(f"  {c['name']}: {a['name']} ({','.join(a['energy_cost'])}) {a['damage']}dmg")
                if attacks_fixed <= 10 or needs_fix.index(c) < 5:
                    continue
                break


if __name__ == "__main__":
    asyncio.run(main())
