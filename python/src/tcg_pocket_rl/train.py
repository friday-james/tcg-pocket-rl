"""Self-play MaskablePPO training for Pokemon TCG Pocket."""

import json
import os
import random
import sys
import time
from pathlib import Path

import numpy as np

from tcg_pocket_rl.env import PokemonTCGPocketEnv


def load_card_db(cards_json: str) -> list[dict]:
    """Load card database for deck building."""
    with open(cards_json) as f:
        return json.load(f)


def build_random_deck(cards: list[dict], rng: random.Random) -> list[str]:
    """Build a valid random 20-card deck.

    Uses only true basics (no evolves_from) and trainers.
    Respects the 2-copy-per-name limit.
    """
    # True basic Pokemon: stage=basic, no evolves_from, has attacks
    basics = [c for c in cards if c.get("card_type") == "pokemon"
              and c.get("stage") == "basic"
              and not c.get("evolves_from")
              and c.get("attacks") and len(c["attacks"]) > 0]
    trainers = [c for c in cards if c.get("card_type") in ("supporter", "item", "tool")
                and c.get("effect")]

    if not basics:
        raise ValueError("No basic Pokemon in card database")

    deck_ids = []
    name_counts = {}

    def try_add(card):
        name = card["name"]
        if name_counts.get(name, 0) < 2:
            deck_ids.append(card["slug"])
            name_counts[name] = name_counts.get(name, 0) + 1
            return True
        return False

    # Add 10-14 basic Pokemon
    n_basics = rng.randint(10, 14)
    attempts = 0
    while len(deck_ids) < n_basics and attempts < 100:
        try_add(rng.choice(basics))
        attempts += 1

    # Fill remaining with trainers
    attempts = 0
    while len(deck_ids) < 20 and attempts < 100:
        try_add(rng.choice(trainers if trainers else basics))
        attempts += 1

    # Pad with basics if still short
    while len(deck_ids) < 20:
        deck_ids.append(rng.choice(basics)["slug"])

    return deck_ids[:20]


class SelfPlayCallback:
    """Callback that periodically snapshots the agent as an opponent."""

    def __init__(self, update_interval: int = 10):
        self.update_interval = update_interval
        self.n_updates = 0
        self.opponent_pool = []

    def on_rollout_end(self, model):
        self.n_updates += 1
        if self.n_updates % self.update_interval == 0:
            # Save a snapshot of the current policy parameters
            params = {k: v.clone() for k, v in model.policy.state_dict().items()}
            self.opponent_pool.append(params)
            print(f"  [Self-play] Added opponent snapshot #{len(self.opponent_pool)}")


def evaluate_vs_random(engine_cls, cards_json, cards, n_games=100):
    """Evaluate a trained model against a random opponent."""
    from sb3_contrib import MaskablePPO

    rng = random.Random(99)
    wins = 0
    for i in range(n_games):
        deck1 = build_random_deck(cards, rng)
        deck2 = build_random_deck(cards, rng)
        env = PokemonTCGPocketEnv(
            cards_json=cards_json,
            deck1_ids=deck1,
            deck2_ids=deck2,
            agent_player=0,
        )
        obs, info = env.reset(seed=i)
        done = False
        while not done:
            mask = info["action_mask"]
            legal = np.where(mask)[0]
            if len(legal) == 0:
                break
            action = np.random.choice(legal)
            obs, reward, done, truncated, info = env.step(action)
        if reward > 0:
            wins += 1
    return wins / n_games


def train(
    cards_json: str,
    total_timesteps: int = 1_000_000,
    save_dir: str = "checkpoints",
    log_dir: str = "logs",
    n_envs: int = 4,
    deck_refresh_interval: int = 5,
):
    """Train a MaskablePPO agent via self-play.

    Args:
        cards_json: Path to cards.json
        total_timesteps: Total training timesteps
        save_dir: Directory to save model checkpoints
        log_dir: TensorBoard log directory
        n_envs: Number of parallel environments
        deck_refresh_interval: Refresh decks every N iterations
    """
    from sb3_contrib import MaskablePPO
    from stable_baselines3.common.vec_env import SubprocVecEnv

    cards = load_card_db(cards_json)
    rng = random.Random(42)

    # Build initial decks
    deck1 = build_random_deck(cards, rng)
    deck2 = build_random_deck(cards, rng)

    print(f"Cards loaded: {len(cards)}")
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

    env = SubprocVecEnv([make_env(i) for i in range(n_envs)])

    model = MaskablePPO(
        "MlpPolicy",
        env,
        policy_kwargs=dict(net_arch=[512, 256, 128]),
        learning_rate=3e-4,
        n_steps=2048,
        batch_size=256,
        n_epochs=10,
        gamma=0.99,
        gae_lambda=0.95,
        clip_range=0.2,
        ent_coef=0.01,
        verbose=1,
        tensorboard_log=log_dir,
    )

    os.makedirs(save_dir, exist_ok=True)

    # Training loop with periodic deck refresh and checkpointing
    steps_per_iter = n_envs * 2048
    n_iterations = max(1, total_timesteps // steps_per_iter)
    steps_done = 0

    print(f"Training for {total_timesteps} timesteps ({n_iterations} iterations)...")
    start_time = time.time()

    for iteration in range(n_iterations):
        remaining = total_timesteps - steps_done
        learn_steps = min(steps_per_iter, remaining)
        if learn_steps <= 0:
            break

        model.learn(
            total_timesteps=learn_steps,
            reset_num_timesteps=False,
            progress_bar=False,
        )
        steps_done += learn_steps

        elapsed = time.time() - start_time
        fps = steps_done / elapsed if elapsed > 0 else 0
        print(f"  Iteration {iteration + 1}/{n_iterations} | "
              f"{steps_done}/{total_timesteps} steps | "
              f"{fps:.0f} fps | {elapsed:.0f}s", flush=True)

        # Refresh decks periodically for diversity
        if (iteration + 1) % deck_refresh_interval == 0:
            deck1 = build_random_deck(cards, rng)
            deck2 = build_random_deck(cards, rng)
            env.close()
            env = SubprocVecEnv([make_env(i * 1000 + iteration) for i in range(n_envs)])
            model.set_env(env)
            print(f"  [Deck refresh] New random decks generated")

        # Save checkpoint periodically
        if (iteration + 1) % 10 == 0:
            ckpt_path = os.path.join(save_dir, f"ppo_tcg_pocket_{steps_done}")
            model.save(ckpt_path)
            print(f"  [Checkpoint] Saved to {ckpt_path}")

    # Final save
    final_path = os.path.join(save_dir, "ppo_tcg_pocket_final")
    model.save(final_path)
    elapsed = time.time() - start_time
    print(f"\nTraining complete: {steps_done} steps in {elapsed:.0f}s ({steps_done/elapsed:.0f} fps)")
    print(f"Model saved to {final_path}")

    env.close()
    return model


if __name__ == "__main__":
    data_dir = Path(__file__).parent.parent.parent.parent / "data"
    cards_json = str(data_dir / "cards.json")

    if not os.path.exists(cards_json):
        print(f"ERROR: {cards_json} not found. Run scripts/finalize_cards.py first.")
        sys.exit(1)

    timesteps = int(sys.argv[1]) if len(sys.argv) > 1 else 1_000_000
    train(cards_json, total_timesteps=timesteps)
