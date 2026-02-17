"""Evolve a deck to beat all 12 meta decks (agent-vs-agent).

Uses genetic algorithm with evolution-aware deck building and evaluates
fitness by playing the trained model on both sides.
"""

import json
import random
import sys
import time
from dataclasses import dataclass
from pathlib import Path

import numpy as np


@dataclass
class DeckCandidate:
    card_ids: list[str]
    fitness: float = 0.0
    matchup_wins: dict = None  # per-meta-deck win counts
    games_played: int = 0

    def __post_init__(self):
        if self.matchup_wins is None:
            self.matchup_wins = {}


# ---------------------------------------------------------------------------
# Card pool & evolution chain helpers
# ---------------------------------------------------------------------------

def build_card_pool(cards):
    """Build card pool with evolution chain mapping.

    Returns:
        basics: list of basic Pokemon cards (with attacks)
        evo_chains: dict mapping basic slug -> [(stage1_slug, ...), (stage2_slug, ...)]
        trainers: list of trainer cards
        slug_to_card: dict slug -> card
        name_to_slugs: dict name -> [slugs]
    """
    slug_to_card = {c["slug"]: c for c in cards}
    name_to_slugs = {}
    for c in cards:
        name_to_slugs.setdefault(c["name"], []).append(c["slug"])

    # Find all basic Pokemon with attacks
    basics = [c for c in cards
              if c.get("card_type") == "pokemon"
              and c.get("stage") == "basic"
              and not c.get("evolves_from")
              and c.get("attacks") and len(c["attacks"]) > 0]

    # Build evolution chains: basic -> stage 1 -> stage 2
    # Map: basic_name -> {stage1_names: [...], stage2_names: [...]}
    evo_map = {}  # basic_name -> list of (stage, card)
    for c in cards:
        if c.get("card_type") != "pokemon":
            continue
        evolves_from = c.get("evolves_from")
        if not evolves_from:
            continue
        stage = c.get("stage", "").lower()
        if "1" in stage:
            # Stage 1: evolves from basic
            evo_map.setdefault(evolves_from, []).append(("stage1", c))
        elif "2" in stage:
            # Stage 2: evolves from stage 1
            # Need to find what basic the stage 1 evolves from
            evo_map.setdefault(evolves_from, []).append(("stage2", c))

    # Build full chains: basic_slug -> [(basic, stage1, stage2?), ...]
    evo_chains = {}
    for basic in basics:
        bname = basic["name"]
        chains = []
        # Find stage 1s that evolve from this basic
        stage1s = [(s, c) for s, c in evo_map.get(bname, []) if s == "stage1"]
        for _, s1 in stage1s:
            # Check if this stage 1 has stage 2s
            stage2s = [(s, c) for s, c in evo_map.get(s1["name"], []) if s == "stage2"]
            if stage2s:
                for _, s2 in stage2s:
                    chains.append((basic["slug"], s1["slug"], s2["slug"]))
            else:
                chains.append((basic["slug"], s1["slug"]))
        if chains:
            evo_chains[basic["slug"]] = chains

    # Trainers with effects
    trainers = [c for c in cards
                if c.get("card_type") in ("supporter", "item", "tool")
                and c.get("effect")]

    return basics, evo_chains, trainers, slug_to_card, name_to_slugs


def build_random_evo_deck(basics, evo_chains, trainers, slug_to_card, rng):
    """Build a random 20-card deck with evolution lines."""
    deck = []
    name_counts = {}

    def can_add(slug):
        name = slug_to_card[slug]["name"]
        return name_counts.get(name, 0) < 2

    def add_card(slug):
        name = slug_to_card[slug]["name"]
        if name_counts.get(name, 0) < 2:
            deck.append(slug)
            name_counts[name] = name_counts.get(name, 0) + 1
            return True
        return False

    # Add 3-5 evolution lines or standalone basics
    n_pokemon_slots = rng.randint(10, 14)

    # Try to add some evolution lines
    evo_basics = [b for b in basics if b["slug"] in evo_chains]
    standalone = [b for b in basics if b["slug"] not in evo_chains]

    rng.shuffle(evo_basics)
    rng.shuffle(standalone)

    # Add 2-4 evolution lines
    n_evo_lines = rng.randint(2, min(4, len(evo_basics)))
    for basic in evo_basics[:n_evo_lines]:
        if len(deck) >= n_pokemon_slots:
            break
        chains = evo_chains[basic["slug"]]
        chain = rng.choice(chains)
        # Add 2 copies of each stage in the chain
        for slug in chain:
            if len(deck) < n_pokemon_slots:
                add_card(slug)
                add_card(slug)

    # Fill remaining pokemon slots with standalone basics
    attempts = 0
    while len(deck) < n_pokemon_slots and attempts < 100:
        basic = rng.choice(standalone if standalone else basics)
        add_card(basic["slug"])
        attempts += 1

    # Fill rest with trainers
    rng.shuffle(trainers)
    attempts = 0
    while len(deck) < 20 and attempts < 200:
        trainer = rng.choice(trainers)
        add_card(trainer["slug"])
        attempts += 1

    # Pad with random basics if still short
    while len(deck) < 20:
        basic = rng.choice(basics)
        deck.append(basic["slug"])

    return deck[:20]


# ---------------------------------------------------------------------------
# Evolution-aware mutation / crossover
# ---------------------------------------------------------------------------

def get_evo_line_slugs(slug, evo_chains, slug_to_card):
    """Get all slugs in the same evolution line as the given slug."""
    card = slug_to_card.get(slug)
    if not card or card.get("card_type") != "pokemon":
        return [slug]

    # Check if this slug is a basic with chains
    if slug in evo_chains:
        # Return all slugs in all chains
        all_slugs = set()
        for chain in evo_chains[slug]:
            all_slugs.update(chain)
        return list(all_slugs)

    # Check if it's part of a chain (stage 1 or stage 2)
    for basic_slug, chains in evo_chains.items():
        for chain in chains:
            if slug in chain:
                return list(chain)

    return [slug]


def mutate_deck(deck_ids, basics, evo_chains, trainers, slug_to_card, rng, n_mutations=2):
    """Mutate a deck, respecting evolution chains."""
    new_deck = list(deck_ids)
    name_counts = {}
    for slug in new_deck:
        card = slug_to_card.get(slug)
        if card:
            name = card["name"]
            name_counts[name] = name_counts.get(name, 0) + 1

    for _ in range(n_mutations):
        if not new_deck:
            break

        idx = rng.randint(0, len(new_deck) - 1)
        removed_slug = new_deck[idx]
        removed_card = slug_to_card.get(removed_slug)
        if removed_card:
            name_counts[removed_card["name"]] = max(0, name_counts.get(removed_card["name"], 1) - 1)

        # Replace with either an evolution line or trainer
        r = rng.random()
        if r < 0.3 and evo_chains:
            # Add a random evolution line member
            basic = rng.choice(list(evo_chains.keys()))
            chain = rng.choice(evo_chains[basic])
            member = rng.choice(chain)
            card = slug_to_card.get(member)
            if card and name_counts.get(card["name"], 0) < 2:
                new_deck[idx] = member
                name_counts[card["name"]] = name_counts.get(card["name"], 0) + 1
                continue
        elif r < 0.6:
            # Add a random basic
            basic = rng.choice(basics)
            if name_counts.get(basic["name"], 0) < 2:
                new_deck[idx] = basic["slug"]
                name_counts[basic["name"]] = name_counts.get(basic["name"], 0) + 1
                continue

        # Add a random trainer
        trainer = rng.choice(trainers)
        if name_counts.get(trainer["name"], 0) < 2:
            new_deck[idx] = trainer["slug"]
            name_counts[trainer["name"]] = name_counts.get(trainer["name"], 0) + 1
        else:
            # Restore
            if removed_card:
                name_counts[removed_card["name"]] = name_counts.get(removed_card["name"], 0) + 1

    return new_deck


def crossover_decks(parent1, parent2, slug_to_card, rng):
    """Uniform crossover preserving name-count limits."""
    child = []
    name_counts = {}

    combined = list(zip(parent1, parent2))
    rng.shuffle(combined)

    for s1, s2 in combined:
        for slug in ([s1, s2] if rng.random() < 0.5 else [s2, s1]):
            card = slug_to_card.get(slug)
            if card and name_counts.get(card["name"], 0) < 2:
                child.append(slug)
                name_counts[card["name"]] = name_counts.get(card["name"], 0) + 1
                break

    # Pad if short
    all_slugs = list(set(parent1 + parent2))
    rng.shuffle(all_slugs)
    for slug in all_slugs:
        if len(child) >= 20:
            break
        card = slug_to_card.get(slug)
        if card and name_counts.get(card["name"], 0) < 2:
            child.append(slug)
            name_counts[card["name"]] = name_counts.get(card["name"], 0) + 1

    return child[:20]


# ---------------------------------------------------------------------------
# Agent-vs-agent fitness evaluation
# ---------------------------------------------------------------------------

def evaluate_deck_vs_meta(
    deck_ids,
    meta_decks,
    meta_names,
    engine,
    model,
    n_games_per_matchup=10,
):
    """Evaluate a deck against all meta decks using agent-vs-agent play.

    Returns (avg_win_rate, per_matchup_wins_dict).
    """
    total_wins = 0
    total_games = 0
    matchup_wins = {}

    for meta_name, meta_deck in zip(meta_names, meta_decks):
        wins = 0
        for game in range(n_games_per_matchup):
            # Alternate who is player 0
            if game % 2 == 0:
                d1, d2 = deck_ids, meta_deck
                candidate_player = 0
            else:
                d1, d2 = meta_deck, deck_ids
                candidate_player = 1

            seed = hash((tuple(deck_ids[:3]), meta_name, game)) % (2**31)

            try:
                engine.reset(d1, d2, seed=seed, agent_player=0)
            except Exception:
                continue

            winner = -1
            for _ in range(500):
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
                    winner = 0 if reward > 0 else 1
                    break

            if winner == candidate_player:
                wins += 1
            total_games += 1

        matchup_wins[meta_name] = wins
        total_wins += wins

    avg_wr = total_wins / total_games if total_games > 0 else 0
    return avg_wr, matchup_wins


def tournament_select(population, rng, k=3):
    """Tournament selection."""
    contestants = rng.sample(population, min(k, len(population)))
    return max(contestants, key=lambda c: c.fitness)


# ---------------------------------------------------------------------------
# Main optimization loop
# ---------------------------------------------------------------------------

def optimize_counter_deck(
    cards_json,
    model_path="checkpoints/ppo_meta_final",
    population_size=50,
    generations=100,
    n_games_per_matchup=10,
    mutation_rate=0.30,
    crossover_rate=0.40,
    elite_ratio=0.10,
):
    import torch
    from sb3_contrib import MaskablePPO
    from tcg_pocket_engine import PyGameEngine

    torch.distributions.Distribution.set_default_validate_args(False)

    # Load card DB
    with open(cards_json) as f:
        cards = json.load(f)

    basics, evo_chains, trainers, slug_to_card, name_to_slugs = build_card_pool(cards)
    print(f"Card pool: {len(basics)} basics, {len(evo_chains)} evo chains, {len(trainers)} trainers")

    # Load meta decks
    sys.path.insert(0, str(Path(__file__).parent))
    from train_meta import resolve_decks
    meta_decks_dict = resolve_decks(cards_json)
    meta_names = list(meta_decks_dict.keys())
    meta_deck_lists = list(meta_decks_dict.values())
    print(f"Loaded {len(meta_deck_lists)} meta decks as opponents")

    # Load trained model
    print(f"Loading model from {model_path}...")
    model = MaskablePPO.load(model_path)
    engine = PyGameEngine(cards_json)

    rng = random.Random(42)
    n_elite = max(1, int(population_size * elite_ratio))

    # Initialize population: seed with meta decks + random
    population = []
    for deck in meta_deck_lists:
        population.append(DeckCandidate(card_ids=list(deck)))

    while len(population) < population_size:
        deck = build_random_evo_deck(basics, evo_chains, trainers, slug_to_card, rng)
        population.append(DeckCandidate(card_ids=deck))

    print(f"\nPopulation: {population_size} (seeded with {len(meta_deck_lists)} meta decks)")
    print(f"Generations: {generations}")
    print(f"Games per matchup: {n_games_per_matchup} (× {len(meta_deck_lists)} decks × 2 sides "
          f"= {n_games_per_matchup * len(meta_deck_lists)} games/deck)")
    print()

    start_time = time.time()
    best_ever = None

    for gen in range(generations):
        # Evaluate unevaluated candidates
        for candidate in population:
            if candidate.games_played == 0:
                try:
                    fitness, matchup_wins = evaluate_deck_vs_meta(
                        candidate.card_ids, meta_deck_lists, meta_names,
                        engine, model, n_games_per_matchup,
                    )
                    candidate.fitness = fitness
                    candidate.matchup_wins = matchup_wins
                    candidate.games_played = n_games_per_matchup * len(meta_deck_lists)
                except Exception as e:
                    candidate.fitness = 0.0
                    candidate.games_played = 1  # mark as evaluated

        # Sort by fitness
        population.sort(key=lambda c: c.fitness, reverse=True)

        best = population[0]
        avg = np.mean([c.fitness for c in population])
        worst_matchup = min(best.matchup_wins.values()) if best.matchup_wins else 0
        elapsed = time.time() - start_time

        if best_ever is None or best.fitness > best_ever.fitness:
            best_ever = DeckCandidate(
                card_ids=list(best.card_ids),
                fitness=best.fitness,
                matchup_wins=dict(best.matchup_wins),
                games_played=best.games_played,
            )

        print(
            f"Gen {gen + 1:3d}/{generations} | "
            f"Best: {best.fitness:.1%} | Avg: {avg:.1%} | "
            f"Worst MU: {worst_matchup}/{n_games_per_matchup} | "
            f"{elapsed:.0f}s",
            flush=True,
        )

        if gen == generations - 1:
            break

        # Build next generation
        new_population = []

        # Elites
        for i in range(n_elite):
            new_population.append(population[i])

        # Offspring
        while len(new_population) < population_size:
            r = rng.random()
            if r < crossover_rate:
                p1 = tournament_select(population, rng)
                p2 = tournament_select(population, rng)
                child = crossover_decks(p1.card_ids, p2.card_ids, slug_to_card, rng)
                new_population.append(DeckCandidate(card_ids=child))
            elif r < crossover_rate + mutation_rate:
                parent = tournament_select(population, rng)
                child = mutate_deck(
                    parent.card_ids, basics, evo_chains, trainers,
                    slug_to_card, rng,
                )
                new_population.append(DeckCandidate(card_ids=child))
            else:
                deck = build_random_evo_deck(basics, evo_chains, trainers, slug_to_card, rng)
                new_population.append(DeckCandidate(card_ids=deck))

        population = new_population

    # === Final Results ===
    print("\n" + "=" * 60)
    print("BEST COUNTER-DECK")
    print("=" * 60)
    print(f"Overall win rate: {best_ever.fitness:.1%}\n")

    # Print cards grouped by type
    card_counts = {}
    for slug in best_ever.card_ids:
        card = slug_to_card.get(slug)
        name = card["name"] if card else slug
        card_counts[name] = card_counts.get(name, 0) + 1

    pokemon_cards = []
    trainer_cards = []
    for slug in best_ever.card_ids:
        card = slug_to_card.get(slug)
        if card:
            if card.get("card_type") == "pokemon":
                pokemon_cards.append(card)
            else:
                trainer_cards.append(card)

    # Deduplicate for display
    seen = set()
    print("Pokemon:")
    for card in pokemon_cards:
        if card["name"] not in seen:
            seen.add(card["name"])
            count = card_counts[card["name"]]
            stage = card.get("stage", "basic")
            hp = card.get("hp", "?")
            etype = card.get("energy_type", "?")
            ex = " ex" if card.get("is_ex") else ""
            print(f"  {count}x {card['name']}{ex} ({stage}, {etype}, {hp}HP)")

    seen = set()
    print("\nTrainers:")
    for card in trainer_cards:
        if card["name"] not in seen:
            seen.add(card["name"])
            count = card_counts[card["name"]]
            ctype = card.get("card_type", "?")
            print(f"  {count}x {card['name']} ({ctype})")

    # Per-matchup breakdown
    print(f"\nMatchup Breakdown ({n_games_per_matchup} games each):")
    for meta_name in meta_names:
        wins = best_ever.matchup_wins.get(meta_name, 0)
        pct = wins / n_games_per_matchup * 100
        bar = "#" * wins + "." * (n_games_per_matchup - wins)
        print(f"  vs {meta_name:25s}: {wins:2d}/{n_games_per_matchup} ({pct:5.1f}%) [{bar}]")

    # Print deck IDs for use
    print(f"\nDeck IDs (for scripts):")
    print(f"  {best_ever.card_ids}")

    return best_ever


if __name__ == "__main__":
    data_dir = Path(__file__).parent.parent / "data"
    cards_json = str(data_dir / "cards.json")

    model_path = sys.argv[1] if len(sys.argv) > 1 else "checkpoints/ppo_meta_final"
    generations = int(sys.argv[2]) if len(sys.argv) > 2 else 100
    pop_size = int(sys.argv[3]) if len(sys.argv) > 3 else 50

    optimize_counter_deck(
        cards_json,
        model_path=model_path,
        generations=generations,
        population_size=pop_size,
    )
