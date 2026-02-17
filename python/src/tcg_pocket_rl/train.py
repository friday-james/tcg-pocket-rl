"""Self-play MaskablePPO training for Pokemon TCG Pocket."""

import json
import os
import random
import sys
from pathlib import Path

import numpy as np

from tcg_pocket_rl.env import PokemonTCGPocketEnv


def load_card_db(cards_json: str) -> list[dict]:
    """Load card database for deck building."""
    with open(cards_json) as f:
        return json.load(f)


def build_random_deck(cards: list[dict], rng: random.Random) -> list[str]:
    """Build a valid random 20-card deck."""
    # Get basic Pokemon
    basics = [c for c in cards if c.get("card_type") == "pokemon"
              and c.get("stage") in ("basic", None)
              and c.get("attacks")]
    trainers = [c for c in cards if c.get("card_type") in ("supporter", "item", "tool")]

    if not basics:
        raise ValueError("No basic Pokemon in card database")

    deck_ids = []
    name_counts = {}

    # Add 8-12 basic Pokemon
    n_basics = rng.randint(8, 12)
    for _ in range(n_basics):
        card = rng.choice(basics)
        name = card["name"]
        if name_counts.get(name, 0) < 2:
            deck_ids.append(card["slug"])
            name_counts[name] = name_counts.get(name, 0) + 1

    # Fill remaining with trainers and more Pokemon
    remaining = 20 - len(deck_ids)
    fillers = basics + trainers
    for _ in range(remaining):
        card = rng.choice(fillers)
        name = card["name"]
        if name_counts.get(name, 0) < 2:
            deck_ids.append(card["slug"])
            name_counts[name] = name_counts.get(name, 0) + 1

    # Pad if needed (shouldn't happen but safety)
    while len(deck_ids) < 20:
        card = rng.choice(basics)
        deck_ids.append(card["slug"])

    return deck_ids[:20]


def train(
    cards_json: str,
    total_timesteps: int = 1_000_000,
    save_dir: str = "checkpoints",
    log_dir: str = "logs",
):
    """Train a MaskablePPO agent via self-play."""
    from sb3_contrib import MaskablePPO
    from stable_baselines3.common.vec_env import SubprocVecEnv

    cards = load_card_db(cards_json)
    rng = random.Random(42)

    # Build initial decks
    deck1 = build_random_deck(cards, rng)
    deck2 = build_random_deck(cards, rng)

    print(f"Deck 1: {len(deck1)} cards")
    print(f"Deck 2: {len(deck2)} cards")

    def make_env(seed):
        def _init():
            env = PokemonTCGPocketEnv(
                cards_json=cards_json,
                deck1_ids=deck1,
                deck2_ids=deck2,
                agent_player=0,
            )
            env._seed = seed
            return env
        return _init

    n_envs = 4
    env = SubprocVecEnv([make_env(i) for i in range(n_envs)])

    model = MaskablePPO(
        "MlpPolicy",
        env,
        policy_kwargs=dict(net_arch=[512, 256, 128]),
        learning_rate=3e-4,
        n_steps=2048,
        batch_size=256,
        n_epochs=10,
        verbose=1,
        tensorboard_log=log_dir,
    )

    os.makedirs(save_dir, exist_ok=True)

    print(f"Training for {total_timesteps} timesteps...")
    model.learn(
        total_timesteps=total_timesteps,
        progress_bar=True,
    )

    model.save(os.path.join(save_dir, "ppo_tcg_pocket"))
    print(f"Model saved to {save_dir}/ppo_tcg_pocket")

    env.close()


if __name__ == "__main__":
    data_dir = Path(__file__).parent.parent.parent.parent / "data"
    cards_json = str(data_dir / "cards.json")

    if not os.path.exists(cards_json):
        print(f"ERROR: {cards_json} not found. Run scripts/finalize_cards.py first.")
        sys.exit(1)

    train(cards_json)
