#!/usr/bin/env python3
"""Merge CDN card data (names, HP, stats) with scraped data (attacks, effects).

CDN data has: name, set, number, rarity, element, type, stage, health, retreatCost, weakness, evolvesFrom
Scraped data has: attacks (with energy costs, damage, effects), abilities, card_type

Match strategy: the scraped slug contains the card name (e.g., "o416iiwlr5ayncv-bulbasaur" -> "bulbasaur")
We match by extracting name from slug and matching to CDN name.
"""

import json
import os
import re
from collections import defaultdict

DATA_DIR = os.path.join(os.path.dirname(__file__), "..", "data")


def slug_to_name(slug: str) -> str:
    """Extract card name from slug: 'o416iiwlr5ayncv-bulbasaur' -> 'Bulbasaur'."""
    # Remove the hash prefix
    parts = slug.split("-", 1)
    if len(parts) > 1:
        name = parts[1]
    else:
        name = slug
    # Convert to title case and replace hyphens
    name = name.replace("-", " ").title()
    # Fix 'Ex' -> 'ex'
    name = re.sub(r"\bEx\b", "ex", name)
    return name


def normalize_name(name: str) -> str:
    """Normalize card name for matching."""
    return name.lower().strip().replace("'", "'").replace("\u2019", "'")


def load_cdn_data():
    """Load and index CDN card data."""
    with open(os.path.join(DATA_DIR, "cards_extra.json")) as f:
        cards = json.load(f)

    # Index by normalized name + set for unique matching
    by_name = defaultdict(list)
    for card in cards:
        name = normalize_name(card.get("name", ""))
        by_name[name].append(card)

    return cards, by_name


def load_scraped_data():
    """Load scraped card data."""
    with open(os.path.join(DATA_DIR, "cards_scraped.json")) as f:
        return json.load(f)


def merge_cards():
    """Merge CDN + scraped data into a complete card database."""
    cdn_cards, cdn_by_name = load_cdn_data()
    scraped_cards = load_scraped_data()

    # Index scraped by normalized name
    scraped_by_name = defaultdict(list)
    for card in scraped_cards:
        name = normalize_name(slug_to_name(card.get("slug", "")))
        scraped_by_name[name].append(card)

    # Build merged database
    merged = []
    matched = 0
    unmatched_cdn = 0
    unmatched_scraped = 0

    # Process CDN cards (these have the authoritative names/stats)
    seen_names = set()
    for cdn_card in cdn_cards:
        name = cdn_card.get("name", "")
        norm_name = normalize_name(name)

        # Skip duplicate names (different prints of same card)
        if norm_name in seen_names:
            continue
        seen_names.add(norm_name)

        # Find matching scraped card
        scraped_matches = scraped_by_name.get(norm_name, [])
        scraped = scraped_matches[0] if scraped_matches else {}

        # Build merged card
        card = {
            "id": scraped.get("slug", f"cdn-{cdn_card.get('set', '')}-{cdn_card.get('number', '')}"),
            "name": name,
            "card_type": scraped.get("card_type") or cdn_card.get("type", "pokemon"),
            "hp": cdn_card.get("health"),
            "stage": normalize_stage(cdn_card.get("stage")),
            "energy_type": normalize_energy(cdn_card.get("element")),
            "weakness": normalize_energy(cdn_card.get("weakness")),
            "retreat_cost": cdn_card.get("retreatCost"),
            "attacks": scraped.get("attacks", []),
            "ability": scraped.get("ability"),
            "evolves_from": cdn_card.get("evolvesFrom"),
            "is_ex": "ex" in name.lower() and name.lower().endswith("ex"),
            "effect": scraped.get("effect"),
            "set_name": cdn_card.get("set"),
            "card_number": cdn_card.get("number"),
            "rarity": cdn_card.get("rarity"),
        }

        if scraped_matches:
            matched += 1
        else:
            unmatched_cdn += 1

        merged.append(card)

    # Also add any scraped-only cards (not in CDN)
    for norm_name, scraped_list in scraped_by_name.items():
        if norm_name not in seen_names:
            for scraped in scraped_list:
                card = {
                    "id": scraped.get("slug", ""),
                    "name": slug_to_name(scraped.get("slug", "")),
                    "card_type": scraped.get("card_type", "pokemon"),
                    "hp": scraped.get("hp"),
                    "stage": scraped.get("stage"),
                    "energy_type": scraped.get("energy_type"),
                    "weakness": scraped.get("weakness"),
                    "retreat_cost": scraped.get("retreat_cost"),
                    "attacks": scraped.get("attacks", []),
                    "ability": scraped.get("ability"),
                    "evolves_from": scraped.get("evolves_from"),
                    "is_ex": scraped.get("is_ex", False),
                    "effect": scraped.get("effect"),
                    "set_name": scraped.get("set_name"),
                    "card_number": scraped.get("card_number"),
                    "rarity": scraped.get("rarity"),
                }
                merged.append(card)
                unmatched_scraped += 1
            seen_names.add(norm_name)

    print(f"Merged: {len(merged)} unique cards")
    print(f"  Matched (CDN + scraped): {matched}")
    print(f"  CDN only (no attack data): {unmatched_cdn}")
    print(f"  Scraped only (no CDN stats): {unmatched_scraped}")

    # Stats
    pokemon = [c for c in merged if c["card_type"] == "pokemon"]
    with_attacks = [c for c in pokemon if c.get("attacks")]
    with_hp = [c for c in pokemon if c.get("hp")]
    trainers = [c for c in merged if c["card_type"] in ("supporter", "item", "tool")]

    print(f"\nPokemon: {len(pokemon)} ({len(with_attacks)} with attacks, {len(with_hp)} with HP)")
    print(f"Trainers: {len(trainers)}")

    # Save
    out_path = os.path.join(DATA_DIR, "cards_complete.json")
    with open(out_path, "w") as f:
        json.dump(merged, f, indent=2)
    print(f"\nSaved to {out_path} ({os.path.getsize(out_path)} bytes)")

    return merged


def normalize_stage(stage):
    if stage is None:
        return None
    s = str(stage).lower()
    if s in ("basic", "0"):
        return "basic"
    if s in ("1", "stage-1", "stage 1"):
        return "stage-1"
    if s in ("2", "stage-2", "stage 2"):
        return "stage-2"
    return s


def normalize_energy(val):
    if val is None:
        return None
    return val.lower()


if __name__ == "__main__":
    merge_cards()
