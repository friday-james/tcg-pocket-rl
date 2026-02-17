#!/usr/bin/env python3
"""Train a Pokemon TCG Pocket RL agent.

Usage:
    python scripts/train_agent.py [timesteps]
    python scripts/train_agent.py 1000000
"""

import os
import sys

# Add python/src to path
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "python", "src"))

from tcg_pocket_rl.train import train

DATA_DIR = os.path.join(os.path.dirname(__file__), "..", "data")


if __name__ == "__main__":
    cards_json = os.path.join(DATA_DIR, "cards.json")

    if not os.path.exists(cards_json):
        print(f"ERROR: {cards_json} not found")
        sys.exit(1)

    timesteps = int(sys.argv[1]) if len(sys.argv) > 1 else 1_000_000
    n_envs = int(sys.argv[2]) if len(sys.argv) > 2 else 4

    print(f"Training for {timesteps} timesteps with {n_envs} envs")
    train(cards_json, total_timesteps=timesteps, n_envs=n_envs)
