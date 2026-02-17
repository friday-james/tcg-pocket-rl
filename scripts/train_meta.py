"""Train RL agent using current meta decks from Pokemon TCG Pocket.

Meta decks sourced from:
- game8.co tier lists
- pokemontcgpocket.app
- ptcgpocket.gg
- pokemonmeta.com

February 2026 meta.
"""

import json
import os
import random
import sys
import time
from pathlib import Path

import numpy as np

# ============================================================================
# META DECKLISTS (20 cards each)
# Card IDs are slugs from the scraped database.
# Multiple printings exist per card; we use the first available ID.
# ============================================================================

# Helper: we'll resolve names to IDs at runtime
META_DECKS_BY_NAME = {
    # =====================================================================
    # S-TIER
    # =====================================================================

    # Pikachu ex evolves from Pichu; Zebstrika from Blitzle
    "Pikachu ex Aggro": {
        "energy": "lightning",
        "cards": [
            ("Pichu", 2),       # Basic -> Pikachu ex
            ("Pikachu ex", 2),  # Stage 1
            ("Zapdos ex", 2),   # Basic
            ("Blitzle", 2),     # Basic -> Zebstrika
            ("Zebstrika", 2),   # Stage 1
            ("Oricorio", 2),    # Basic (tech)
            ("Volkner", 2),
            ("Giovanni", 2),
            ("Pok\u00e9 Ball", 2),
            ("Professor\u2019s Research", 2),
        ],
    },

    "Mewtwo ex Control": {
        "energy": "psychic",
        "cards": [
            ("Mewtwo ex", 2),   # Basic
            ("Ralts", 2),       # Basic -> Kirlia -> Gardevoir
            ("Kirlia", 2),      # Stage 1
            ("Gardevoir", 2),   # Stage 2
            ("Jirachi", 2),     # Basic
            ("Giovanni", 2),
            ("Sabrina", 2),
            ("Professor\u2019s Research", 2),
            ("Pok\u00e9 Ball", 2),
            ("X Speed", 2),
        ],
    },

    # Greninja ex evolves from Frogadier (from Froakie)
    "Suicune Greninja": {
        "energy": "water",
        "cards": [
            ("Suicune ex", 2),  # Basic
            ("Froakie", 2),     # Basic -> Frogadier -> Greninja ex
            ("Frogadier", 2),   # Stage 1
            ("Greninja ex", 2), # Stage 2
            ("Mantyke", 2),     # Basic
            ("Misty", 2),
            ("Professor\u2019s Research", 2),
            ("Pok\u00e9 Ball", 2),
            ("Sabrina", 2),
            ("X Speed", 2),
        ],
    },

    # Charizard ex from Charmeleon from Charmander; Arcanine ex from Growlithe
    "Charizard ex": {
        "energy": "fire",
        "cards": [
            ("Charmander", 2),  # Basic -> Charmeleon -> Charizard ex
            ("Charmeleon", 2),  # Stage 1
            ("Charizard ex", 2),# Stage 2
            ("Moltres ex", 2),  # Basic
            ("Growlithe", 2),   # Basic -> Arcanine ex
            ("Arcanine ex", 2), # Stage 1
            ("Giovanni", 2),
            ("Professor\u2019s Research", 2),
            ("Pok\u00e9 Ball", 2),
            ("Sabrina", 2),
        ],
    },

    # =====================================================================
    # A-TIER
    # =====================================================================

    # Venusaur evolves from Ivysaur (from Bulbasaur)
    "Celebi Exeggutor": {
        "energy": "grass",
        "cards": [
            ("Celebi ex", 2),    # Basic
            ("Exeggcute", 2),    # Basic -> Exeggutor ex
            ("Exeggutor ex", 2), # Stage 1
            ("Bulbasaur", 2),    # Basic -> Ivysaur -> Venusaur
            ("Ivysaur", 2),      # Stage 1
            ("Venusaur", 2),     # Stage 2
            ("Erika", 2),
            ("Professor\u2019s Research", 2),
            ("Pok\u00e9 Ball", 2),
            ("Sabrina", 2),
        ],
    },

    "Starmie ex": {
        "energy": "water",
        "cards": [
            ("Staryu", 2),      # Basic -> Starmie ex
            ("Starmie ex", 2),  # Stage 1
            ("Suicune ex", 2),  # Basic
            ("Articuno ex", 2), # Basic
            ("Mantyke", 2),     # Basic
            ("Misty", 2),
            ("Professor\u2019s Research", 2),
            ("Pok\u00e9 Ball", 2),
            ("Sabrina", 2),
            ("X Speed", 2),
        ],
    },

    # Hydreigon from Zweilous from Deino; Mega Absol from Absol
    "Darkrai Hydreigon": {
        "energy": "darkness",
        "cards": [
            ("Darkrai ex", 2),    # Basic
            ("Deino", 2),         # Basic -> Zweilous -> Hydreigon
            ("Zweilous", 2),      # Stage 1
            ("Hydreigon", 2),     # Stage 2
            ("Absol", 2),         # Basic -> Mega Absol ex
            ("Mega Absol ex", 2), # Stage 1 (Mega)
            ("Giovanni", 2),
            ("Sabrina", 2),
            ("Professor\u2019s Research", 2),
            ("Pok\u00e9 Ball", 2),
        ],
    },

    "Dialga Palkia": {
        "energy": "water",
        "cards": [
            ("Dialga ex", 2),   # Basic
            ("Palkia ex", 2),   # Basic
            ("Suicune ex", 2),  # Basic
            ("Articuno ex", 2), # Basic
            ("Mantyke", 2),     # Basic
            ("Misty", 2),
            ("Sabrina", 2),
            ("Professor\u2019s Research", 2),
            ("Pok\u00e9 Ball", 2),
            ("Giovanni", 2),
        ],
    },

    # =====================================================================
    # B-TIER
    # =====================================================================

    # Lucario ex from Riolu; Aerodactyl ex from Old Amber (fossil)
    "Buzzwole Lucario": {
        "energy": "fighting",
        "cards": [
            ("Buzzwole ex", 2),   # Basic
            ("Riolu", 2),         # Basic -> Lucario ex
            ("Lucario ex", 2),    # Stage 1
            ("Oricorio", 2),      # Basic
            ("Jirachi", 2),       # Basic
            ("Brock", 2),
            ("Giovanni", 2),
            ("Professor\u2019s Research", 2),
            ("Pok\u00e9 Ball", 2),
            ("Sabrina", 2),
        ],
    },

    "Mew Mewtwo": {
        "energy": "psychic",
        "cards": [
            ("Mew ex", 2),     # Basic
            ("Mewtwo ex", 2),  # Basic
            ("Ralts", 2),      # Basic -> Kirlia -> Gardevoir
            ("Kirlia", 2),     # Stage 1
            ("Gardevoir", 2),  # Stage 2
            ("Jirachi", 2),    # Basic
            ("Sabrina", 2),
            ("Professor\u2019s Research", 2),
            ("Pok\u00e9 Ball", 2),
            ("Giovanni", 2),
        ],
    },

    # Gyarados ex from Magikarp
    "Gyarados ex": {
        "energy": "water",
        "cards": [
            ("Magikarp", 2),    # Basic -> Gyarados ex
            ("Gyarados ex", 2), # Stage 1
            ("Staryu", 2),      # Basic -> Starmie ex
            ("Starmie ex", 2),  # Stage 1
            ("Suicune ex", 2),  # Basic
            ("Mantyke", 2),     # Basic
            ("Misty", 2),
            ("Sabrina", 2),
            ("Professor\u2019s Research", 2),
            ("Pok\u00e9 Ball", 2),
        ],
    },

    # Mega Altaria ex from Swablu (Altaria is separate evo line, both from Swablu)
    "Mega Altaria": {
        "energy": "psychic",
        "cards": [
            ("Swablu", 2),          # Basic -> Mega Altaria ex
            ("Mega Altaria ex", 2), # Stage 1 (Mega)
            ("Chingling", 2),       # Basic
            ("Jirachi", 2),         # Basic
            ("Oricorio", 2),        # Basic
            ("Sabrina", 2),
            ("Professor\u2019s Research", 2),
            ("Pok\u00e9 Ball", 2),
            ("Giovanni", 2),
            ("X Speed", 2),
        ],
    },
}


def resolve_decks(cards_json: str) -> dict[str, list[str]]:
    """Resolve card names to IDs from the database."""
    with open(cards_json) as f:
        cards = json.load(f)

    # Build name -> first slug mapping
    name_to_slug = {}
    for c in cards:
        name = c["name"]
        if name not in name_to_slug:
            name_to_slug[name] = c["slug"]

    resolved = {}
    for deck_name, deck_info in META_DECKS_BY_NAME.items():
        deck_ids = []
        missing = []
        for card_name, count in deck_info["cards"]:
            slug = name_to_slug.get(card_name)
            if slug:
                deck_ids.extend([slug] * count)
            else:
                missing.append(card_name)

        if missing:
            print(f"  WARNING: {deck_name} missing cards: {missing}")

        if len(deck_ids) == 20:
            resolved[deck_name] = deck_ids
        else:
            print(f"  WARNING: {deck_name} has {len(deck_ids)} cards (need 20), skipping")

    return resolved


def train_meta(
    cards_json: str,
    total_timesteps: int = 2_000_000,
    save_dir: str = "checkpoints",
    log_dir: str = "logs",
    n_envs: int = 8,
    resume_from: str | None = None,
):
    """Train RL agent using meta deck matchups.

    Instead of random decks, uses actual meta decks for both the agent
    and opponent, cycling through all matchup combinations.
    """
    import torch
    from sb3_contrib import MaskablePPO
    from stable_baselines3.common.vec_env import SubprocVecEnv
    from tcg_pocket_rl.env import PokemonTCGPocketEnv

    # Disable strict distribution validation — MaskablePPO's softmax
    # occasionally fails the Simplex tolerance check due to float32 precision.
    torch.distributions.Distribution.set_default_validate_args(False)

    print("Resolving meta decks...")
    meta_decks = resolve_decks(cards_json)
    deck_names = list(meta_decks.keys())
    deck_lists = list(meta_decks.values())

    print(f"Loaded {len(deck_lists)} meta decks:")
    for name in deck_names:
        print(f"  - {name}")

    if len(deck_lists) < 2:
        print("ERROR: Need at least 2 meta decks")
        sys.exit(1)

    rng = random.Random(42)

    # Pick initial matchup
    agent_deck = deck_lists[0]
    opp_deck = deck_lists[1]

    def make_env(seed, d1, d2, player):
        def _init():
            env = PokemonTCGPocketEnv(
                cards_json=cards_json,
                deck1_ids=d1,
                deck2_ids=d2,
                agent_player=player,
            )
            env._seed = seed
            return env
        return _init

    def create_envs(d1, d2, base_seed=0):
        """Create n_envs environments, half as player 0 and half as player 1."""
        envs = []
        for i in range(n_envs):
            player = i % 2  # Alternate player sides
            envs.append(make_env(base_seed + i, d1, d2, player))
        return SubprocVecEnv(envs)

    env = create_envs(agent_deck, opp_deck)

    os.makedirs(save_dir, exist_ok=True)

    steps_per_iter = n_envs * 2048
    n_iterations = max(1, total_timesteps // steps_per_iter)
    steps_done = 0
    matchup_idx = 0

    # Generate all matchup pairs
    matchups = []
    for i in range(len(deck_lists)):
        for j in range(len(deck_lists)):
            if i != j:
                matchups.append((i, j))
    rng.shuffle(matchups)

    if resume_from:
        print(f"Resuming from checkpoint: {resume_from}")
        model = MaskablePPO.load(resume_from, env=env)
        # Parse steps done from checkpoint filename (e.g., ppo_meta_655360)
        try:
            steps_done = int(Path(resume_from).stem.split("_")[-1])
        except (ValueError, IndexError):
            steps_done = 0
        matchup_idx = (steps_done // (steps_per_iter * 3)) % len(matchups)
        print(f"  Resuming from step {steps_done}, matchup index {matchup_idx}")
    else:
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

    print(f"\nTraining for {total_timesteps} timesteps ({n_iterations} iterations)")
    print(f"Total matchup pairs: {len(matchups)}")
    print(f"Cycling through matchups every 3 iterations\n")
    start_time = time.time()

    start_iteration = steps_done // steps_per_iter

    for iteration in range(start_iteration, n_iterations):
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

        # Rotate matchup every 3 iterations
        if (iteration + 1) % 3 == 0:
            matchup_idx = (matchup_idx + 1) % len(matchups)
            i, j = matchups[matchup_idx]
            agent_deck = deck_lists[i]
            opp_deck = deck_lists[j]
            env.close()
            env = create_envs(agent_deck, opp_deck, base_seed=iteration * 100)
            model.set_env(env)
            print(f"  [{steps_done}] Matchup: {deck_names[i]} vs {deck_names[j]}")

        if (iteration + 1) % 10 == 0:
            print(f"  Iteration {iteration + 1}/{n_iterations} | "
                  f"{steps_done}/{total_timesteps} steps | "
                  f"{fps:.0f} fps | {elapsed:.0f}s", flush=True)

        # Save checkpoint periodically
        if (iteration + 1) % 20 == 0:
            ckpt_path = os.path.join(save_dir, f"ppo_meta_{steps_done}")
            model.save(ckpt_path)
            print(f"  [Checkpoint] Saved to {ckpt_path}")

    # Final save
    final_path = os.path.join(save_dir, "ppo_meta_final")
    model.save(final_path)
    elapsed = time.time() - start_time
    print(f"\nTraining complete: {steps_done} steps in {elapsed:.0f}s ({steps_done/elapsed:.0f} fps)")
    print(f"Model saved to {final_path}")

    env.close()

    # Evaluate against each meta deck
    print("\n=== META DECK EVALUATION ===")
    evaluate_meta(cards_json, final_path, meta_decks, deck_names)

    return model


def evaluate_meta(cards_json, model_path, meta_decks, deck_names, n_games=50):
    """Evaluate trained model on all meta matchups."""
    from sb3_contrib import MaskablePPO
    from tcg_pocket_rl.env import PokemonTCGPocketEnv

    model = MaskablePPO.load(model_path)
    deck_lists = list(meta_decks.values())

    results = {}
    for i, name_i in enumerate(deck_names):
        wins = 0
        total = 0
        for j, name_j in enumerate(deck_names):
            if i == j:
                continue
            for game in range(n_games):
                player = game % 2
                env = PokemonTCGPocketEnv(
                    cards_json=cards_json,
                    deck1_ids=deck_lists[i],
                    deck2_ids=deck_lists[j],
                    agent_player=player,
                )
                obs, info = env.reset(seed=game + i * 1000 + j * 100)
                done = False
                while not done:
                    mask = info.get("action_mask")
                    if mask is not None:
                        action, _ = model.predict(obs, action_masks=mask, deterministic=True)
                    else:
                        action, _ = model.predict(obs, deterministic=True)
                    obs, reward, done, truncated, info = env.step(int(action))
                if reward > 0:
                    wins += 1
                total += 1

        win_rate = wins / total if total > 0 else 0
        results[name_i] = win_rate
        print(f"  {name_i}: {win_rate:.1%} win rate ({wins}/{total})")

    # Sort by win rate
    print("\n=== TIER RANKING ===")
    for rank, (name, wr) in enumerate(sorted(results.items(), key=lambda x: -x[1]), 1):
        tier = "S" if wr >= 0.6 else "A" if wr >= 0.5 else "B" if wr >= 0.4 else "C"
        print(f"  {rank}. [{tier}] {name}: {wr:.1%}")


if __name__ == "__main__":
    data_dir = Path(__file__).parent.parent / "data"
    cards_json = str(data_dir / "cards.json")

    if not os.path.exists(cards_json):
        print(f"ERROR: {cards_json} not found")
        sys.exit(1)

    timesteps = int(sys.argv[1]) if len(sys.argv) > 1 else 2_000_000
    n_envs = int(sys.argv[2]) if len(sys.argv) > 2 else 8
    resume = sys.argv[3] if len(sys.argv) > 3 else None

    train_meta(cards_json, total_timesteps=timesteps, n_envs=n_envs, resume_from=resume)
