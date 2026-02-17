#!/usr/bin/env python3
"""Build final card database by merging scraped data with CDN metadata.

Sources:
- data/cards_final.json: Website RSC scrape (attacks, HP, name from schema.org)
- data/cards_extra.json: CDN (card_type, evolvesFrom, stage)
- data/sets.json: Set code -> name mapping

Output: data/cards.json (final engine-ready database)
"""

import json
import os
import re
from collections import Counter

DATA_DIR = os.path.join(os.path.dirname(__file__), "..", "data")


def load_json(filename):
    path = os.path.join(DATA_DIR, filename)
    with open(path) as f:
        return json.load(f)


def build_set_mapping(sets_data):
    """Build set code -> English name mapping."""
    mapping = {}
    for series in sets_data.values():
        for s in series:
            mapping[s["code"]] = s["name"]["en"]
    return mapping


def build_cdn_index(cdn_extra, set_mapping):
    """Build lookup: (set_name, card_number) -> CDN card data."""
    index = {}
    for card in cdn_extra:
        set_code = card.get("set", "")
        set_name = set_mapping.get(set_code, set_code)
        number = card.get("number", 0)
        key = (set_name, number)
        index[key] = card
    return index


def normalize_card_type(card, cdn_card):
    """Determine the correct card type using CDN data."""
    if cdn_card:
        t = cdn_card.get("type", "").lower()
        if t in ("supporter", "item", "tool", "fossil"):
            return t
        if t == "pokemon":
            return "pokemon"
    # Fallback: cards without HP are trainers
    if not card.get("hp"):
        return "item"
    return card.get("card_type", "pokemon")


def normalize_stage(stage_str):
    """Normalize stage strings."""
    if not stage_str:
        return None
    s = stage_str.lower().strip()
    if s in ("basic",):
        return "basic"
    if s in ("stage-1", "stage 1", "stage1"):
        return "stage 1"
    if s in ("stage-2", "stage 2", "stage2"):
        return "stage 2"
    return None


def normalize_energy(energy_str):
    """Normalize energy type strings."""
    if not energy_str:
        return None
    e = energy_str.lower().strip()
    mapping = {
        "grass": "grass",
        "fire": "fire",
        "water": "water",
        "lightning": "lightning",
        "electric": "lightning",
        "psychic": "psychic",
        "fighting": "fighting",
        "darkness": "darkness",
        "dark": "darkness",
        "metal": "metal",
        "steel": "metal",
        "dragon": "dragon",
        "colorless": "colorless",
        "normal": "colorless",
    }
    return mapping.get(e)


def main():
    scraped = load_json("cards_final.json")
    cdn_extra = load_json("cards_extra.json")
    sets_data = load_json("sets.json")

    set_mapping = build_set_mapping(sets_data)
    cdn_index = build_cdn_index(cdn_extra, set_mapping)

    print(f"Scraped: {len(scraped)} cards")
    print(f"CDN extra: {len(cdn_extra)} cards")
    print(f"Sets: {len(set_mapping)}")

    matched = 0
    cards = []

    for sc in scraped:
        set_name = sc.get("set_name")
        card_number = sc.get("card_number")

        # Try to find CDN match
        cdn_card = cdn_index.get((set_name, card_number))
        if cdn_card:
            matched += 1

        card_type = normalize_card_type(sc, cdn_card)

        # Build final card
        card = {
            "slug": sc["slug"],
            "url": sc.get("url"),
            "name": sc.get("name", ""),
            "card_type": card_type,
        }

        if card_type == "pokemon":
            card["hp"] = sc.get("hp")
            card["stage"] = normalize_stage(
                sc.get("stage") or (cdn_card.get("stage") if cdn_card else None)
            )
            card["energy_type"] = normalize_energy(sc.get("energy_type"))
            card["weakness"] = normalize_energy(sc.get("weakness"))
            card["retreat_cost"] = sc.get("retreat_cost")
            card["attacks"] = sc.get("attacks", [])
            card["ability"] = None  # TODO: fix RSC ability parsing
            card["evolves_from"] = (
                cdn_card.get("evolvesFrom") if cdn_card else sc.get("evolves_from")
            )
            card["is_ex"] = sc.get("is_ex", False)
        else:
            card["hp"] = None
            card["stage"] = None
            card["energy_type"] = None
            card["weakness"] = None
            card["retreat_cost"] = None
            card["attacks"] = []
            card["ability"] = None
            card["evolves_from"] = None
            card["is_ex"] = False
            card["effect"] = None  # TODO: fix trainer effect parsing

        card["set_name"] = set_name
        card["card_number"] = card_number
        card["rarity"] = sc.get("rarity")

        # Clean up attack names (remove tabs/extra whitespace)
        for atk in card.get("attacks", []):
            atk["name"] = atk["name"].strip().replace("\t", "")

        cards.append(card)

    # Deduplicate: keep first occurrence of each (name, set_name, card_number)
    seen = set()
    unique_cards = []
    for c in cards:
        key = (c["name"], c["set_name"], c["card_number"])
        if key not in seen:
            seen.add(key)
            unique_cards.append(c)

    # Sort by set, then card number
    unique_cards.sort(key=lambda c: (c.get("set_name") or "", c.get("card_number") or 0))

    # Save
    output_path = os.path.join(DATA_DIR, "cards.json")
    with open(output_path, "w") as f:
        json.dump(unique_cards, f, indent=2)

    # Stats
    types = Counter(c["card_type"] for c in unique_cards)
    pokemon = [c for c in unique_cards if c["card_type"] == "pokemon"]
    with_attacks = sum(1 for c in pokemon if c.get("attacks"))
    with_hp = sum(1 for c in pokemon if c.get("hp") and c["hp"] > 0)
    with_evo = sum(1 for c in pokemon if c.get("evolves_from"))
    exs = sum(1 for c in pokemon if c.get("is_ex"))

    print(f"\nCDN matched: {matched}/{len(scraped)}")
    print(f"\nFinal database: {len(unique_cards)} unique cards")
    print(f"  Card types: {dict(types)}")
    print(f"  Pokemon: {len(pokemon)} ({with_attacks} with attacks, {with_hp} with HP)")
    print(f"  EX: {exs}")
    print(f"  Evolves: {with_evo}")
    print(f"  Saved to {output_path}")

    # Validate: check for common issues
    issues = []
    for c in unique_cards:
        if c["card_type"] == "pokemon":
            if not c.get("hp") or c["hp"] == 0:
                issues.append(f"Pokemon with no HP: {c['name']}")
            if not c.get("attacks"):
                pass  # Some Pokemon only have abilities
            if not c.get("stage"):
                issues.append(f"Pokemon with no stage: {c['name']}")

    if issues:
        print(f"\nWarnings ({len(issues)}):")
        for issue in issues[:10]:
            print(f"  {issue}")
        if len(issues) > 10:
            print(f"  ... and {len(issues) - 10} more")


if __name__ == "__main__":
    main()
