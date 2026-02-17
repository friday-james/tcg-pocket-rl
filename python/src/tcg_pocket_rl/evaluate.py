"""Deck evaluation and matchup analysis."""

import json
import random

import numpy as np

from tcg_pocket_rl.env import PokemonTCGPocketEnv
from tcg_pocket_rl.train import build_random_deck, load_card_db


def evaluate_matchup(
    deck1_ids: list[str],
    deck2_ids: list[str],
    cards_json: str,
    model,
    n_games: int = 100,
) -> dict:
    """Evaluate deck1 vs deck2 matchup.

    Returns dict with win rates for both sides.
    """
    wins_as_p0 = 0
    wins_as_p1 = 0

    for i in range(n_games):
        for agent_player in [0, 1]:
            if agent_player == 0:
                d1, d2 = deck1_ids, deck2_ids
            else:
                d1, d2 = deck2_ids, deck1_ids

            env = PokemonTCGPocketEnv(
                cards_json=cards_json,
                deck1_ids=d1,
                deck2_ids=d2,
                agent_player=agent_player,
            )

            obs, info = env.reset(seed=i * 2 + agent_player)
            done = False
            for _ in range(500):
                mask = info["action_mask"]
                if mask.sum() == 0:
                    break
                action, _ = model.predict(obs, action_masks=mask, deterministic=False)
                obs, reward, done, _, info = env.step(action)
                if done:
                    if reward > 0:
                        if agent_player == 0:
                            wins_as_p0 += 1
                        else:
                            wins_as_p1 += 1
                    break

    total_games = n_games * 2
    total_wins = wins_as_p0 + wins_as_p1
    return {
        "deck1_win_rate": total_wins / total_games,
        "deck1_wins_as_p0": wins_as_p0,
        "deck1_wins_as_p1": wins_as_p1,
        "total_games": total_games,
    }


def describe_deck(deck_ids: list[str], cards_json: str) -> str:
    """Return a human-readable description of a deck."""
    cards = load_card_db(cards_json)
    slug_to_card = {c["slug"]: c for c in cards}

    pokemon = []
    trainers = []
    for slug in deck_ids:
        card = slug_to_card.get(slug, {"name": slug, "card_type": "unknown"})
        if card.get("card_type") == "pokemon":
            pokemon.append(card)
        else:
            trainers.append(card)

    lines = []
    lines.append(f"Deck ({len(deck_ids)} cards):")
    lines.append(f"  Pokemon ({len(pokemon)}):")

    # Group by name
    name_counts = {}
    for c in pokemon:
        name = c["name"]
        name_counts[name] = name_counts.get(name, 0) + 1
    for name, count in sorted(name_counts.items()):
        card = next(c for c in pokemon if c["name"] == name)
        hp = card.get("hp", "?")
        energy = card.get("energy_type", "?")
        suffix = f" x{count}" if count > 1 else ""
        lines.append(f"    {name} ({energy}, {hp}HP){suffix}")

    lines.append(f"  Trainers ({len(trainers)}):")
    name_counts = {}
    for c in trainers:
        name = c["name"]
        name_counts[name] = name_counts.get(name, 0) + 1
    for name, count in sorted(name_counts.items()):
        suffix = f" x{count}" if count > 1 else ""
        lines.append(f"    {name}{suffix}")

    return "\n".join(lines)
