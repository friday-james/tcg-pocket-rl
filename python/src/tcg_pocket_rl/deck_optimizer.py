"""Evolutionary deck optimization using RL agent evaluation."""

import json
import random
import time
from dataclasses import dataclass, field

import numpy as np

from tcg_pocket_rl.env import PokemonTCGPocketEnv
from tcg_pocket_rl.train import build_random_deck, load_card_db


@dataclass
class DeckCandidate:
    card_ids: list[str]
    fitness: float = 0.0
    games_played: int = 0


def evaluate_deck(
    deck_ids: list[str],
    cards_json: str,
    model,
    n_games: int = 50,
    opponent_decks: list[list[str]] | None = None,
    cards: list[dict] | None = None,
) -> float:
    """Evaluate a deck by playing games with the RL agent.

    Returns win rate as fitness score.
    """
    rng = random.Random(hash(tuple(deck_ids)))
    if cards is None:
        cards = load_card_db(cards_json)

    wins = 0
    for i in range(n_games):
        # Play as both player 0 and player 1 for fairness
        for agent_player in [0, 1]:
            if opponent_decks:
                opp_deck = rng.choice(opponent_decks)
            else:
                opp_deck = build_random_deck(cards, rng)

            if agent_player == 0:
                d1, d2 = deck_ids, opp_deck
            else:
                d1, d2 = opp_deck, deck_ids

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
                        wins += 1
                    break

    return wins / (n_games * 2)


def mutate_deck(
    deck_ids: list[str],
    card_pool: list[dict],
    rng: random.Random,
    n_mutations: int = 2,
) -> list[str]:
    """Mutate a deck by swapping random cards."""
    new_deck = list(deck_ids)
    name_counts = {}
    slug_to_name = {}

    for card in card_pool:
        slug_to_name[card["slug"]] = card["name"]

    for slug in new_deck:
        name = slug_to_name.get(slug, slug)
        name_counts[name] = name_counts.get(name, 0) + 1

    for _ in range(n_mutations):
        if not new_deck:
            break
        # Remove a random card
        idx = rng.randint(0, len(new_deck) - 1)
        removed_slug = new_deck[idx]
        removed_name = slug_to_name.get(removed_slug, removed_slug)
        name_counts[removed_name] = name_counts.get(removed_name, 1) - 1

        # Add a random valid card from the pool
        candidates = [
            c for c in card_pool
            if name_counts.get(c["name"], 0) < 2
        ]
        if candidates:
            new_card = rng.choice(candidates)
            new_deck[idx] = new_card["slug"]
            name_counts[new_card["name"]] = name_counts.get(new_card["name"], 0) + 1
        else:
            # Restore if no valid replacement
            name_counts[removed_name] = name_counts.get(removed_name, 0) + 1

    return new_deck


def crossover_decks(
    parent1: list[str],
    parent2: list[str],
    card_pool: list[dict],
    rng: random.Random,
) -> list[str]:
    """Create a child deck from two parents using uniform crossover."""
    slug_to_card = {c["slug"]: c for c in card_pool}
    child = []
    name_counts = {}

    # Shuffle combined cards
    combined = list(zip(parent1, parent2))
    rng.shuffle(combined)

    for s1, s2 in combined:
        # Try to add from either parent
        for slug in ([s1, s2] if rng.random() < 0.5 else [s2, s1]):
            card = slug_to_card.get(slug)
            if card and name_counts.get(card["name"], 0) < 2:
                child.append(slug)
                name_counts[card["name"]] = name_counts.get(card["name"], 0) + 1
                break

    # Pad to 20 if needed
    while len(child) < 20:
        candidates = [c for c in card_pool if name_counts.get(c["name"], 0) < 2]
        if candidates:
            c = rng.choice(candidates)
            child.append(c["slug"])
            name_counts[c["name"]] = name_counts.get(c["name"], 0) + 1
        else:
            break

    return child[:20]


def optimize_deck(
    cards_json: str,
    model,
    population_size: int = 50,
    generations: int = 100,
    n_eval_games: int = 30,
    mutation_rate: float = 0.3,
    crossover_rate: float = 0.4,
    elite_ratio: float = 0.1,
    card_pool: list[dict] | None = None,
    seed_decks: list[list[str]] | None = None,
) -> list[DeckCandidate]:
    """Find optimal deck using evolutionary algorithm.

    Args:
        cards_json: Path to cards.json
        model: Trained MaskablePPO model
        population_size: Number of decks per generation
        generations: Number of evolutionary generations
        n_eval_games: Games per deck per evaluation
        mutation_rate: Probability of mutation
        crossover_rate: Probability of crossover
        elite_ratio: Fraction of top decks kept unchanged
        card_pool: Cards available for deck building (defaults to all)
        seed_decks: Initial decks to include in population

    Returns:
        Sorted list of DeckCandidates (best first)
    """
    cards = load_card_db(cards_json)
    rng = random.Random(42)

    if card_pool is None:
        # All Pokemon with attacks + all trainers with effects
        card_pool = [
            c for c in cards
            if (c.get("card_type") == "pokemon" and c.get("stage") == "basic"
                and not c.get("evolves_from") and c.get("attacks"))
            or (c.get("card_type") in ("supporter", "item", "tool") and c.get("effect"))
        ]

    # Initialize population
    population = []
    if seed_decks:
        for deck in seed_decks:
            population.append(DeckCandidate(card_ids=deck))

    while len(population) < population_size:
        deck = build_random_deck(cards, rng)
        population.append(DeckCandidate(card_ids=deck))

    n_elite = max(1, int(population_size * elite_ratio))
    start_time = time.time()

    for gen in range(generations):
        # Evaluate fitness
        for candidate in population:
            if candidate.games_played == 0:
                candidate.fitness = evaluate_deck(
                    candidate.card_ids, cards_json, model,
                    n_games=n_eval_games, cards=cards,
                )
                candidate.games_played = n_eval_games

        # Sort by fitness (descending)
        population.sort(key=lambda c: c.fitness, reverse=True)

        best = population[0]
        avg = np.mean([c.fitness for c in population])
        elapsed = time.time() - start_time

        print(
            f"  Gen {gen + 1}/{generations} | "
            f"Best: {best.fitness:.3f} | Avg: {avg:.3f} | "
            f"{elapsed:.0f}s",
            flush=True,
        )

        if gen == generations - 1:
            break

        # Selection + reproduction
        new_population = []

        # Keep elites unchanged
        for i in range(n_elite):
            new_population.append(population[i])

        # Fill rest with offspring
        while len(new_population) < population_size:
            r = rng.random()

            if r < crossover_rate:
                # Tournament selection for parents
                p1 = tournament_select(population, rng)
                p2 = tournament_select(population, rng)
                child_ids = crossover_decks(p1.card_ids, p2.card_ids, card_pool, rng)
                new_population.append(DeckCandidate(card_ids=child_ids))

            elif r < crossover_rate + mutation_rate:
                # Mutate a tournament-selected parent
                parent = tournament_select(population, rng)
                child_ids = mutate_deck(parent.card_ids, card_pool, rng)
                new_population.append(DeckCandidate(card_ids=child_ids))

            else:
                # Fresh random deck
                deck = build_random_deck(cards, rng)
                new_population.append(DeckCandidate(card_ids=deck))

        population = new_population

    population.sort(key=lambda c: c.fitness, reverse=True)
    return population


def tournament_select(
    population: list[DeckCandidate],
    rng: random.Random,
    tournament_size: int = 3,
) -> DeckCandidate:
    """Select a candidate using tournament selection."""
    contestants = rng.sample(population, min(tournament_size, len(population)))
    return max(contestants, key=lambda c: c.fitness)
