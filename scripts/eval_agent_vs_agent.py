"""Agent-vs-Agent evaluation across all meta deck matchups.

Both players use the trained MaskablePPO model (with correct per-player
observations) to select actions. This reveals true deck strength.
"""

import sys
from pathlib import Path

import numpy as np


def run_agent_vs_agent_game(engine, model, deck1_ids, deck2_ids, seed):
    """Run a single game where both players use the trained model.

    Returns 0 if deck1 wins, 1 if deck2 wins, -1 if draw/timeout.
    """
    engine.reset(deck1_ids, deck2_ids, seed=seed, agent_player=0)

    for _ in range(1000):  # safety limit
        if engine.is_done():
            break

        current = engine.current_player()

        # Get observation from the current player's perspective
        obs = np.array(engine.observation_for(current), dtype=np.float32)
        mask = np.array(engine.action_masks(), dtype=np.bool_)
        if not mask.any():
            mask[0] = True

        action, _ = model.predict(obs, action_masks=mask, deterministic=True)
        action = int(action)

        try:
            engine.step(action)
        except ValueError:
            # Invalid action — pick first legal
            legal = engine.legal_action_indices()
            if legal:
                engine.step(legal[0])
            else:
                try:
                    engine.step(114)  # end turn
                except ValueError:
                    return -1  # broken state

    if engine.is_done():
        # Determine winner: step() returns reward relative to agent_player=0
        # We just check who won via the engine state
        # Since agent_player=0, a positive final reward means player 0 won
        obs = engine.observation()  # triggers final state check
        # Use current_player after game over to infer winner
        # Actually, let's just play a dummy step to get the reward
        pass

    return -1  # timeout


def evaluate_agent_vs_agent(cards_json, model_path, n_games=50):
    """Run agent-vs-agent evaluation on all meta matchups."""
    import torch
    from sb3_contrib import MaskablePPO
    from tcg_pocket_engine import PyGameEngine

    torch.distributions.Distribution.set_default_validate_args(False)

    sys.path.insert(0, str(Path(__file__).parent))
    from train_meta import resolve_decks

    print("Resolving meta decks...")
    meta_decks = resolve_decks(cards_json)
    deck_names = list(meta_decks.keys())
    deck_lists = list(meta_decks.values())
    print(f"Loaded {len(deck_lists)} meta decks\n")

    print(f"Loading model from {model_path}...")
    model = MaskablePPO.load(model_path)

    engine = PyGameEngine(cards_json)

    n = len(deck_lists)
    wins = [[0] * n for _ in range(n)]
    total_games = n * (n - 1) * n_games

    print(f"Running {total_games} agent-vs-agent games "
          f"({n_games} per matchup, {n*(n-1)} matchups)...\n", flush=True)

    games_done = 0
    for i in range(n):
        for j in range(n):
            if i == j:
                continue

            matchup_wins_i = 0
            for game in range(n_games):
                # Alternate who is player 0 (goes first half the time)
                if game % 2 == 0:
                    d1, d2 = deck_lists[i], deck_lists[j]
                    i_is_player = 0
                else:
                    d1, d2 = deck_lists[j], deck_lists[i]
                    i_is_player = 1

                seed = game + i * 10000 + j * 100

                engine.reset(d1, d2, seed=seed, agent_player=0)

                winner = -1
                for _ in range(1000):
                    if engine.is_done():
                        break

                    current = engine.current_player()
                    obs = np.array(engine.observation_for(current), dtype=np.float32)
                    mask = np.array(engine.action_masks(), dtype=np.bool_)
                    if not mask.any():
                        mask[0] = True

                    action, _ = model.predict(obs, action_masks=mask, deterministic=True)
                    action = int(action)

                    try:
                        _, reward, done, _, _ = engine.step(action)
                    except ValueError:
                        legal = engine.legal_action_indices()
                        if legal:
                            try:
                                _, reward, done, _, _ = engine.step(legal[0])
                            except ValueError:
                                break
                        else:
                            break

                    if done:
                        # reward is from agent_player=0's perspective
                        # reward > 0 means player 0 won
                        if reward > 0:
                            winner = 0
                        elif reward < 0:
                            winner = 1
                        break

                if winner == i_is_player:
                    matchup_wins_i += 1

                games_done += 1

            wins[i][j] = matchup_wins_i
            pct = matchup_wins_i / n_games * 100
            print(f"  {deck_names[i]:25s} vs {deck_names[j]:25s}: "
                  f"{matchup_wins_i:2d}/{n_games} ({pct:5.1f}%)", flush=True)

        # Print running total after each deck's matchups
        total_w = sum(wins[i][jj] for jj in range(n) if jj != i)
        total_g = (n - 1) * n_games
        print(f"  >> {deck_names[i]} overall: {total_w}/{total_g} "
              f"({total_w/total_g*100:.1f}%)\n", flush=True)

    # Final results
    print("\n" + "=" * 60)
    print("OVERALL DECK WIN RATES (Agent vs Agent)")
    print("=" * 60)

    results = {}
    for i in range(n):
        total_w = sum(wins[i][j] for j in range(n) if j != i)
        total_g = (n - 1) * n_games
        wr = total_w / total_g
        results[deck_names[i]] = (wr, total_w, total_g)

    sorted_results = sorted(results.items(), key=lambda x: -x[1][0])

    for rank, (name, (wr, w, t)) in enumerate(sorted_results, 1):
        if wr >= 0.60:
            tier = "S"
        elif wr >= 0.52:
            tier = "A"
        elif wr >= 0.48:
            tier = "B"
        elif wr >= 0.40:
            tier = "C"
        else:
            tier = "D"
        print(f"  {rank:2d}. [{tier}] {name:25s}: {wr:.1%} ({w}/{t})")

    # Matchup matrix
    print("\n" + "=" * 60)
    print("MATCHUP MATRIX (row wins vs column, out of " + str(n_games) + ")")
    print("=" * 60)

    short = [name[:10] for name in deck_names]
    header = f"{'':>25s} | " + " | ".join(f"{s:>10s}" for s in short)
    print(header)
    print("-" * len(header))

    for i in range(n):
        row = f"{deck_names[i]:>25s} | "
        cells = []
        for j in range(n):
            if i == j:
                cells.append(f"{'---':>10s}")
            else:
                pct = wins[i][j] / n_games * 100
                cells.append(f"{wins[i][j]:>2d} ({pct:4.0f}%)")
        row += " | ".join(cells)
        print(row)


if __name__ == "__main__":
    data_dir = Path(__file__).parent.parent / "data"
    cards_json = str(data_dir / "cards.json")

    model_path = sys.argv[1] if len(sys.argv) > 1 else "checkpoints/ppo_meta_final"
    n_games = int(sys.argv[2]) if len(sys.argv) > 2 else 50

    evaluate_agent_vs_agent(cards_json, model_path, n_games=n_games)
