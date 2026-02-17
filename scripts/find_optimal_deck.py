#!/usr/bin/env python3
"""Find the optimal deck using evolutionary optimization + RL evaluation.

Usage:
    python scripts/find_optimal_deck.py [model_path]
    python scripts/find_optimal_deck.py checkpoints/ppo_tcg_pocket_final

Constrained examples:
    # Fire-only deck
    python scripts/find_optimal_deck.py --type fire

    # Budget deck (no cards above Rare)
    python scripts/find_optimal_deck.py --max-rarity R

    # Specific set only
    python scripts/find_optimal_deck.py --set "Genetic Apex"
"""

import argparse
import json
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "python", "src"))

from sb3_contrib import MaskablePPO

from tcg_pocket_rl.constraints import DeckConstraints
from tcg_pocket_rl.deck_optimizer import optimize_deck
from tcg_pocket_rl.evaluate import describe_deck
from tcg_pocket_rl.train import load_card_db

DATA_DIR = os.path.join(os.path.dirname(__file__), "..", "data")


def main():
    parser = argparse.ArgumentParser(description="Find optimal Pokemon TCG Pocket deck")
    parser.add_argument("model_path", nargs="?", default="checkpoints/ppo_tcg_pocket_final")
    parser.add_argument("--type", help="Energy type restriction (fire, water, grass, etc.)")
    parser.add_argument("--set", help="Set name restriction")
    parser.add_argument("--max-rarity", help="Maximum rarity (C, U, R, RR, AR, SR, SAR, IM, CR)")
    parser.add_argument("--population", type=int, default=50)
    parser.add_argument("--generations", type=int, default=50)
    parser.add_argument("--eval-games", type=int, default=30)
    parser.add_argument("--collection", help="Path to JSON file with owned card slugs")
    args = parser.parse_args()

    cards_json = os.path.join(DATA_DIR, "cards.json")
    if not os.path.exists(cards_json):
        print(f"ERROR: {cards_json} not found")
        sys.exit(1)

    if not os.path.exists(args.model_path + ".zip"):
        print(f"ERROR: Model not found at {args.model_path}")
        print("Train an agent first: python scripts/train_agent.py")
        sys.exit(1)

    # Build constraints
    constraints = DeckConstraints()

    if args.type:
        constraints.allowed_types = {args.type.lower()}

    if args.set:
        constraints.allowed_sets = {args.set}

    if args.max_rarity:
        constraints.max_rarity = args.max_rarity

    if args.collection:
        with open(args.collection) as f:
            constraints.available_cards = set(json.load(f))

    # Filter card pool
    cards = load_card_db(cards_json)
    if any([constraints.allowed_types, constraints.allowed_sets,
            constraints.max_rarity, constraints.available_cards]):
        # Apply constraints to card pool, but only filter pokemon by type
        # (trainers/supporters are type-neutral)
        card_pool = []
        for c in cards:
            if c.get("card_type") == "pokemon":
                if not constraints._card_passes(c):
                    continue
                if c.get("stage") != "basic" or c.get("evolves_from"):
                    continue
                if not c.get("attacks"):
                    continue
            elif c.get("card_type") in ("supporter", "item", "tool"):
                if not c.get("effect"):
                    continue
                # Check non-type constraints for trainers too
                if constraints.available_cards and c["slug"] not in constraints.available_cards:
                    continue
                if constraints.allowed_sets and c.get("set_name") not in constraints.allowed_sets:
                    continue
            else:
                continue
            card_pool.append(c)

        print(f"Card pool after constraints: {len(card_pool)} cards")
        if len(card_pool) < 20:
            print("ERROR: Not enough cards to build a deck with these constraints")
            sys.exit(1)
    else:
        card_pool = None

    # Load model
    print(f"Loading model from {args.model_path}...")
    model = MaskablePPO.load(args.model_path)

    # Optimize
    print(f"Optimizing deck ({args.population} pop, {args.generations} gens, {args.eval_games} eval games)...")
    results = optimize_deck(
        cards_json=cards_json,
        model=model,
        population_size=args.population,
        generations=args.generations,
        n_eval_games=args.eval_games,
        card_pool=card_pool,
    )

    # Show results
    print("\n" + "=" * 50)
    print("TOP 5 DECKS")
    print("=" * 50)
    for i, candidate in enumerate(results[:5]):
        print(f"\n--- #{i + 1} (Win Rate: {candidate.fitness:.1%}) ---")
        print(describe_deck(candidate.card_ids, cards_json))

    # Save best deck
    best = results[0]
    output = {
        "deck": best.card_ids,
        "win_rate": best.fitness,
        "constraints": {
            "type": args.type,
            "set": args.set,
            "max_rarity": args.max_rarity,
        },
    }
    output_path = os.path.join(DATA_DIR, "optimal_deck.json")
    with open(output_path, "w") as f:
        json.dump(output, f, indent=2)
    print(f"\nBest deck saved to {output_path}")


if __name__ == "__main__":
    main()
