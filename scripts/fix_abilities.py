#!/usr/bin/env python3
"""Fix ability data using correct RSC pattern.

The ability section in RSC has this structure:
  "children":"Ability" (label in red box)
  "text-lg text-red-700 font-bold","children":"ABILITY_NAME"
  "text-sm pt-1","children":"ABILITY_DESCRIPTION"

Also fixes trainer effects using schema.org description, and
extracts HP from schema.org for cards missing it.
"""

import asyncio
import json
import os
import re
import time

from playwright.async_api import async_playwright

DATA_DIR = os.path.join(os.path.dirname(__file__), "..", "data")
BASE_URL = "https://pocket.pokemongohub.net"


def parse_ability_correct(rsc_text: str) -> dict | None:
    """Extract ability using the correct RSC pattern.

    Abilities appear with a red-700 colored header, distinct from attacks (which use regular text).
    """
    # Look for the "Ability" label, then the ability name in red-700
    m = re.search(
        r'"children":"Ability"\}'
        r'.*?'
        r'"text-lg\s+text-red-700\s+font-bold","children":"([^"]+)"'
        r'.*?'
        r'"text-sm\s+pt-1","children":"([^"]+)"',
        rsc_text[:40000],
        re.DOTALL,
    )
    if m:
        name = m.group(1).strip()
        desc = m.group(2).strip()
        return {"name": name, "description": desc}

    # Alternative: sometimes the class order differs
    m = re.search(
        r'"children":\["Ability"\]'
        r'.*?'
        r'"font-bold","children":"([^"]+)"'
        r'.*?'
        r'"text-sm[^"]*","children":"([^"]{10,})"',
        rsc_text[:40000],
        re.DOTALL,
    )
    if m:
        return {"name": m.group(1).strip(), "description": m.group(2).strip()}

    return None


def parse_effect_from_description(rsc_text: str) -> str | None:
    """Extract trainer card effect from schema.org description."""
    schema_start = rsc_text.find('{"@context":"https://schema.org"')
    if schema_start < 0:
        return None

    depth = 0
    for i in range(schema_start, min(schema_start + 5000, len(rsc_text))):
        if rsc_text[i] == '{':
            depth += 1
        elif rsc_text[i] == '}':
            depth -= 1
            if depth == 0:
                try:
                    schema = json.loads(rsc_text[schema_start:i + 1])
                    desc = schema.get("description", "")
                    # Strip prefix: "CardName - Rarity card #N from SetName. "
                    m = re.match(r'^[^.]+\.\s*(.+)', desc)
                    if m:
                        effect = m.group(1).strip()
                        # Remove "You may play only 1 Supporter card during your turn." suffix
                        effect = re.sub(
                            r'\s*You may play only 1 Supporter card during your turn\.',
                            '',
                            effect,
                        ).strip()
                        if effect and len(effect) > 5:
                            return effect
                except json.JSONDecodeError:
                    pass
                break
    return None


def parse_hp_from_schema(rsc_text: str) -> int | None:
    """Extract HP from schema.org additionalProperty."""
    schema_start = rsc_text.find('{"@context":"https://schema.org"')
    if schema_start < 0:
        return None

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

    # First, clear all false ability and effect assignments
    for c in cards:
        if c.get("ability") and c["ability"].get("name") == "Home":
            c["ability"] = None
        if c["card_type"] != "pokemon" and c.get("effect"):
            bad = ["This page could not be found", "This set contains", "Cost to craft"]
            if any(b in c["effect"] for b in bad):
                c["effect"] = None

    # Collect all card URLs
    urls_list = sorted(set(c["url"] for c in cards))
    print(f"Fetching {len(urls_list)} RSC payloads...", flush=True)

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

        BATCH_SIZE = 20
        rsc_data = {}
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
            if done % 500 == 0 or done == len(urls_list):
                elapsed = time.time() - start
                print(f"  [{done}/{len(urls_list)}] {elapsed:.0f}s", flush=True)

        await browser.close()

    elapsed = time.time() - start
    print(f"Fetched {len(rsc_data)} RSC payloads in {elapsed:.0f}s", flush=True)

    # Parse and fix
    ability_count = 0
    effect_count = 0
    hp_count = 0

    for card in cards:
        slug = card["slug"]
        if slug not in rsc_data:
            continue
        rsc = rsc_data[slug]

        # Fix abilities for Pokemon
        if card["card_type"] == "pokemon" and not card.get("ability"):
            ability = parse_ability_correct(rsc)
            if ability:
                card["ability"] = ability
                ability_count += 1

        # Fix effects for trainers
        if card["card_type"] != "pokemon" and not card.get("effect"):
            effect = parse_effect_from_description(rsc)
            if effect:
                card["effect"] = effect
                effect_count += 1

        # Fix HP
        if card["card_type"] == "pokemon" and (not card.get("hp") or card["hp"] == 0):
            hp = parse_hp_from_schema(rsc)
            if hp:
                card["hp"] = hp
                hp_count += 1

    # Save
    with open(cards_path, "w") as f:
        json.dump(cards, f, indent=2)

    # Stats
    pokemon = [c for c in cards if c["card_type"] == "pokemon"]
    with_ability = sum(1 for c in pokemon if c.get("ability"))
    no_hp = sum(1 for c in pokemon if not c.get("hp") or c["hp"] == 0)
    no_attacks = sum(1 for c in pokemon if not c.get("attacks") or len(c["attacks"]) == 0)
    trainers = [c for c in cards if c["card_type"] != "pokemon"]
    no_effect = sum(1 for c in trainers if not c.get("effect"))

    print(f"\nFixes applied:")
    print(f"  Abilities found: {ability_count}")
    print(f"  Effects found: {effect_count}")
    print(f"  HP found: {hp_count}")

    print(f"\nFinal stats:")
    print(f"  Pokemon with abilities: {with_ability}/{len(pokemon)}")
    print(f"  Pokemon without HP: {no_hp}")
    print(f"  Pokemon without attacks: {no_attacks}")
    print(f"  Trainers without effects: {no_effect}")

    # Show ability examples
    if with_ability > 0:
        print(f"\nAbility examples:")
        for c in pokemon:
            if c.get("ability"):
                print(f"  {c['name']}: {c['ability']['name']} - {c['ability']['description'][:80]}")
                if sum(1 for _ in filter(lambda x: x.get('ability'), pokemon[:pokemon.index(c)+1])) >= 5:
                    break


if __name__ == "__main__":
    asyncio.run(main())
