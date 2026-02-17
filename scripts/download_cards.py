#!/usr/bin/env python3
"""Download Pokemon TCG Pocket card data from CDN."""

import json
import os
import requests

CDN_BASE = "https://cdn.jsdelivr.net/npm/pokemon-tcg-pocket-database/dist"
DATA_DIR = os.path.join(os.path.dirname(__file__), "..", "data")

FILES = {
    "cards.json": f"{CDN_BASE}/cards.json",
    "cards_extra.json": f"{CDN_BASE}/cards.extra.json",
    "sets.json": f"{CDN_BASE}/sets.json",
}


def download():
    os.makedirs(DATA_DIR, exist_ok=True)
    for filename, url in FILES.items():
        path = os.path.join(DATA_DIR, filename)
        print(f"Downloading {filename}...")
        resp = requests.get(url, timeout=30)
        resp.raise_for_status()
        with open(path, "w") as f:
            json.dump(resp.json(), f, indent=2)
        print(f"  Saved to {path} ({os.path.getsize(path)} bytes)")

    # Merge cards.json + cards_extra.json into cards_merged.json
    print("Merging card data...")
    with open(os.path.join(DATA_DIR, "cards.json")) as f:
        cards = json.load(f)
    with open(os.path.join(DATA_DIR, "cards_extra.json")) as f:
        extra = json.load(f)

    # cards_extra.json has the same structure but with additional fields
    # Merge by matching set+number
    extra_lookup = {}
    for card in extra:
        key = f"{card.get('set', '')}-{card.get('number', '')}"
        extra_lookup[key] = card

    merged = []
    for card in cards:
        key = f"{card.get('set', '')}-{card.get('number', '')}"
        if key in extra_lookup:
            merged_card = {**card, **extra_lookup[key]}
        else:
            merged_card = card
        merged.append(merged_card)

    out_path = os.path.join(DATA_DIR, "cards_merged.json")
    with open(out_path, "w") as f:
        json.dump(merged, f, indent=2)
    print(f"Merged {len(merged)} cards into {out_path}")


if __name__ == "__main__":
    download()
