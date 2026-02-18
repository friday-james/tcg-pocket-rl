"""Evolve a deck to beat all 12 meta decks (agent-vs-agent).

Uses genetic algorithm with evolution-aware deck building and evaluates
fitness by playing the trained model on both sides.

Key constraint: all Pokemon must be usable with a single Energy Zone type.
"""

import json
import random
import sys
import time
from dataclasses import dataclass
from pathlib import Path

import numpy as np


ENERGY_TYPES = ["water", "fire", "grass", "lightning", "psychic",
                "fighting", "darkness", "metal"]


@dataclass
class DeckCandidate:
    card_ids: list[str]
    energy_type: str = ""
    fitness: float = 0.0
    matchup_wins: dict = None
    games_played: int = 0

    def __post_init__(self):
        if self.matchup_wins is None:
            self.matchup_wins = {}


# ---------------------------------------------------------------------------
# Energy-aware card pool
# ---------------------------------------------------------------------------

def pokemon_usable_with_energy(card, energy_type):
    """Check if a Pokemon's attacks can be powered by the given energy type.

    Colorless costs can be paid by any energy. A Pokemon is usable if ALL
    its attacks only require the chosen energy type + colorless.
    """
    if card.get("card_type") != "pokemon":
        return False
    attacks = card.get("attacks", [])
    if not attacks:
        return False
    for atk in attacks:
        for cost in atk.get("energy_cost", []):
            if cost in ("colorless", "empty", "normal"):
                continue
            if cost != energy_type:
                return False
    return True


def build_card_pool(cards, energy_type):
    """Build card pool for a specific energy type.

    Returns:
        pokemon: list of usable Pokemon cards
        evo_chains: dict basic_slug -> [chain_tuples]
        trainers: list of trainer cards
        slug_to_card: full slug->card map
    """
    slug_to_card = {c["slug"]: c for c in cards}

    # Pokemon usable with this energy type
    usable_pokemon = [c for c in cards
                      if pokemon_usable_with_energy(c, energy_type)]

    usable_names = {c["name"] for c in usable_pokemon}

    # Basics (no evolves_from, has attacks)
    basics = [c for c in usable_pokemon
              if c.get("stage") == "basic"
              and not c.get("evolves_from")]

    # Build evolution chains among usable Pokemon
    evo_map = {}
    for c in usable_pokemon:
        evolves_from = c.get("evolves_from")
        if not evolves_from:
            continue
        stage = c.get("stage", "").lower()
        if "1" in stage:
            evo_map.setdefault(evolves_from, []).append(("stage1", c))
        elif "2" in stage:
            evo_map.setdefault(evolves_from, []).append(("stage2", c))

    evo_chains = {}
    for basic in basics:
        bname = basic["name"]
        chains = []
        stage1s = [(s, c) for s, c in evo_map.get(bname, []) if s == "stage1"]
        for _, s1 in stage1s:
            stage2s = [(s, c) for s, c in evo_map.get(s1["name"], []) if s == "stage2"]
            if stage2s:
                for _, s2 in stage2s:
                    chains.append((basic["slug"], s1["slug"], s2["slug"]))
            else:
                chains.append((basic["slug"], s1["slug"]))
        if chains:
            evo_chains[basic["slug"]] = chains

    trainers = [c for c in cards
                if c.get("card_type") in ("supporter", "item", "tool")
                and c.get("effect")]

    return basics, evo_chains, trainers, slug_to_card


# ---------------------------------------------------------------------------
# Deck building
# ---------------------------------------------------------------------------

def build_random_evo_deck(basics, evo_chains, trainers, slug_to_card, rng):
    """Build a random 20-card deck with evolution lines."""
    deck = []
    name_counts = {}

    def add_card(slug):
        card = slug_to_card.get(slug)
        if not card:
            return False
        name = card["name"]
        if name_counts.get(name, 0) < 2:
            deck.append(slug)
            name_counts[name] = name_counts.get(name, 0) + 1
            return True
        return False

    n_pokemon_slots = rng.randint(10, 14)

    evo_basics = [b for b in basics if b["slug"] in evo_chains]
    standalone = [b for b in basics if b["slug"] not in evo_chains]
    rng.shuffle(evo_basics)
    rng.shuffle(standalone)

    # Add 2-4 evolution lines (2 copies each)
    n_evo_lines = rng.randint(2, min(4, max(1, len(evo_basics))))
    for basic in evo_basics[:n_evo_lines]:
        if len(deck) >= n_pokemon_slots:
            break
        chains = evo_chains[basic["slug"]]
        chain = rng.choice(chains)
        for slug in chain:
            if len(deck) < n_pokemon_slots:
                add_card(slug)
                add_card(slug)

    # Fill remaining pokemon with standalone basics
    attempts = 0
    while len(deck) < n_pokemon_slots and attempts < 100:
        pool = standalone if standalone else basics
        add_card(rng.choice(pool)["slug"])
        attempts += 1

    # Fill rest with trainers
    attempts = 0
    while len(deck) < 20 and attempts < 200:
        add_card(rng.choice(trainers)["slug"])
        attempts += 1

    # Pad if still short
    while len(deck) < 20:
        deck.append(rng.choice(basics)["slug"])

    return deck[:20]


# ---------------------------------------------------------------------------
# Mutation / crossover
# ---------------------------------------------------------------------------

def mutate_deck(deck_ids, basics, evo_chains, trainers, slug_to_card, rng, n_mutations=2):
    """Mutate a deck, respecting energy type and evolution chains."""
    new_deck = list(deck_ids)
    name_counts = {}
    for slug in new_deck:
        card = slug_to_card.get(slug)
        if card:
            name_counts[card["name"]] = name_counts.get(card["name"], 0) + 1

    for _ in range(n_mutations):
        if not new_deck:
            break

        idx = rng.randint(0, len(new_deck) - 1)
        removed = slug_to_card.get(new_deck[idx])
        if removed:
            name_counts[removed["name"]] = max(0, name_counts.get(removed["name"], 1) - 1)

        r = rng.random()
        added = False
        if r < 0.3 and evo_chains:
            basic_slug = rng.choice(list(evo_chains.keys()))
            chain = rng.choice(evo_chains[basic_slug])
            member_slug = rng.choice(chain)
            card = slug_to_card.get(member_slug)
            if card and name_counts.get(card["name"], 0) < 2:
                new_deck[idx] = member_slug
                name_counts[card["name"]] = name_counts.get(card["name"], 0) + 1
                added = True
        elif r < 0.6 and basics:
            basic = rng.choice(basics)
            if name_counts.get(basic["name"], 0) < 2:
                new_deck[idx] = basic["slug"]
                name_counts[basic["name"]] = name_counts.get(basic["name"], 0) + 1
                added = True

        if not added and trainers:
            trainer = rng.choice(trainers)
            if name_counts.get(trainer["name"], 0) < 2:
                new_deck[idx] = trainer["slug"]
                name_counts[trainer["name"]] = name_counts.get(trainer["name"], 0) + 1
            elif removed:
                name_counts[removed["name"]] = name_counts.get(removed["name"], 0) + 1

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
# Agent-vs-agent evaluation
# ---------------------------------------------------------------------------

def evaluate_deck_vs_meta(deck_ids, meta_decks, meta_names, engine, model, n_games=10):
    """Evaluate a deck against all meta decks (agent-vs-agent)."""
    total_wins = 0
    total_games = 0
    matchup_wins = {}

    for meta_name, meta_deck in zip(meta_names, meta_decks):
        wins = 0
        for game in range(n_games):
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

                try:
                    _, reward, done, _, _ = engine.step(int(action))
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
    contestants = rng.sample(population, min(k, len(population)))
    return max(contestants, key=lambda c: c.fitness)


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def optimize_counter_deck(
    cards_json,
    model_path="checkpoints/ppo_meta_final",
    population_size=50,
    generations=30,
    n_games_per_matchup=10,
    mutation_rate=0.30,
    crossover_rate=0.40,
    elite_ratio=0.10,
):
    import torch
    from sb3_contrib import MaskablePPO
    from tcg_pocket_engine import PyGameEngine

    torch.distributions.Distribution.set_default_validate_args(False)

    with open(cards_json) as f:
        cards = json.load(f)

    slug_to_card = {c["slug"]: c for c in cards}

    # Load meta decks
    sys.path.insert(0, str(Path(__file__).parent))
    from train_meta import resolve_decks, META_DECKS_BY_NAME
    meta_decks_dict = resolve_decks(cards_json)
    meta_names = list(meta_decks_dict.keys())
    meta_deck_lists = list(meta_decks_dict.values())

    # Determine energy type for each meta deck
    meta_energy = {}
    for name, info in META_DECKS_BY_NAME.items():
        meta_energy[name] = info["energy"]

    print(f"Loaded {len(meta_deck_lists)} meta decks as opponents")

    # Load model
    print(f"Loading model from {model_path}...")
    model = MaskablePPO.load(model_path)
    engine = PyGameEngine(cards_json)

    rng = random.Random(42)
    n_elite = max(1, int(population_size * elite_ratio))

    # Build per-energy-type card pools
    energy_pools = {}
    for etype in ENERGY_TYPES:
        basics, evo_chains, trainers, _ = build_card_pool(cards, etype)
        energy_pools[etype] = (basics, evo_chains, trainers)
        n_evo = sum(len(chains) for chains in evo_chains.values())
        print(f"  {etype:10s}: {len(basics):3d} basics, {n_evo:3d} evo lines, {len(trainers)} trainers")

    # Initialize population: seed with meta decks + random per-type decks
    population = []
    for name, deck in zip(meta_names, meta_deck_lists):
        etype = meta_energy.get(name, "water")
        population.append(DeckCandidate(card_ids=list(deck), energy_type=etype))

    while len(population) < population_size:
        etype = rng.choice(ENERGY_TYPES)
        basics, evo_chains, trainers = energy_pools[etype]
        if not basics:
            continue
        deck = build_random_evo_deck(basics, evo_chains, trainers, slug_to_card, rng)
        population.append(DeckCandidate(card_ids=deck, energy_type=etype))

    print(f"\nPopulation: {population_size} | Generations: {generations}")
    print(f"Games per matchup: {n_games_per_matchup} "
          f"(× {len(meta_deck_lists)} = {n_games_per_matchup * len(meta_deck_lists)} games/deck)\n")

    start_time = time.time()
    best_ever = None

    for gen in range(generations):
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
                    candidate.games_played = 1

        population.sort(key=lambda c: c.fitness, reverse=True)

        best = population[0]
        avg = np.mean([c.fitness for c in population])
        worst_mu = min(best.matchup_wins.values()) if best.matchup_wins else 0
        elapsed = time.time() - start_time

        if best_ever is None or best.fitness > best_ever.fitness:
            best_ever = DeckCandidate(
                card_ids=list(best.card_ids),
                energy_type=best.energy_type,
                fitness=best.fitness,
                matchup_wins=dict(best.matchup_wins),
                games_played=best.games_played,
            )

        print(
            f"Gen {gen + 1:3d}/{generations} | "
            f"Best: {best.fitness:.1%} ({best.energy_type}) | "
            f"Avg: {avg:.1%} | Worst MU: {worst_mu}/{n_games_per_matchup} | "
            f"{elapsed:.0f}s",
            flush=True,
        )

        if gen == generations - 1:
            break

        # Next generation
        new_population = []
        for i in range(n_elite):
            new_population.append(population[i])

        while len(new_population) < population_size:
            r = rng.random()
            if r < crossover_rate:
                p1 = tournament_select(population, rng)
                p2 = tournament_select(population, rng)
                # Crossover only between same energy type
                if p1.energy_type == p2.energy_type:
                    child = crossover_decks(p1.card_ids, p2.card_ids, slug_to_card, rng)
                    new_population.append(DeckCandidate(card_ids=child, energy_type=p1.energy_type))
                else:
                    # Pick the better parent's type, mutate it
                    parent = p1 if p1.fitness > p2.fitness else p2
                    etype = parent.energy_type
                    basics, evo_chains, trainers = energy_pools[etype]
                    child = mutate_deck(parent.card_ids, basics, evo_chains, trainers, slug_to_card, rng)
                    new_population.append(DeckCandidate(card_ids=child, energy_type=etype))

            elif r < crossover_rate + mutation_rate:
                parent = tournament_select(population, rng)
                etype = parent.energy_type
                basics, evo_chains, trainers = energy_pools[etype]
                child = mutate_deck(parent.card_ids, basics, evo_chains, trainers, slug_to_card, rng)
                new_population.append(DeckCandidate(card_ids=child, energy_type=etype))

            else:
                etype = rng.choice(ENERGY_TYPES)
                basics, evo_chains, trainers = energy_pools[etype]
                if not basics:
                    continue
                deck = build_random_evo_deck(basics, evo_chains, trainers, slug_to_card, rng)
                new_population.append(DeckCandidate(card_ids=deck, energy_type=etype))

        population = new_population

    # === Results ===
    print("\n" + "=" * 60)
    print(f"BEST COUNTER-DECK (Energy Zone: {best_ever.energy_type.upper()})")
    print("=" * 60)
    print(f"Overall win rate: {best_ever.fitness:.1%}\n")

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

    seen = set()
    print("Pokemon:")
    for card in pokemon_cards:
        if card["name"] not in seen:
            seen.add(card["name"])
            count = card_counts[card["name"]]
            stage = card.get("stage", "basic")
            hp = card.get("hp", "?")
            etype = card.get("energy_type", "?")
            ex_str = " (EX)" if card.get("is_ex") else ""
            evolves = f" [from {card['evolves_from']}]" if card.get("evolves_from") else ""
            print(f"  {count}x {card['name']}{ex_str} — {stage}, {etype}, {hp}HP{evolves}")
            for atk in card.get("attacks", []):
                cost = "+".join(atk.get("energy_cost", [])) or "free"
                dmg = atk.get("damage", 0)
                eff = atk.get("effect", "")
                print(f"     -> {atk['name']} [{cost}] {dmg} dmg" + (f" | {eff}" if eff else ""))
            if card.get("ability"):
                ab = card["ability"]
                print(f"     ** Ability: {ab.get('name','?')} — {ab.get('description','?')}")

    seen = set()
    print("\nTrainers:")
    for card in trainer_cards:
        if card["name"] not in seen:
            seen.add(card["name"])
            count = card_counts[card["name"]]
            ctype = card.get("card_type", "?")
            effect = card.get("effect", "?")
            # Truncate long effects
            if len(effect) > 80:
                effect = effect[:77] + "..."
            print(f"  {count}x {card['name']} ({ctype}) — {effect}")

    print(f"\nMatchup Breakdown ({n_games_per_matchup} games each):")
    for meta_name in meta_names:
        wins = best_ever.matchup_wins.get(meta_name, 0)
        pct = wins / n_games_per_matchup * 100
        bar = "#" * wins + "." * (n_games_per_matchup - wins)
        print(f"  vs {meta_name:25s}: {wins:2d}/{n_games_per_matchup} ({pct:5.1f}%) [{bar}]")

    print(f"\nDeck IDs: {best_ever.card_ids}")
    return best_ever


if __name__ == "__main__":
    data_dir = Path(__file__).parent.parent / "data"
    cards_json = str(data_dir / "cards.json")

    model_path = sys.argv[1] if len(sys.argv) > 1 else "checkpoints/ppo_meta_final"
    generations = int(sys.argv[2]) if len(sys.argv) > 2 else 30
    pop_size = int(sys.argv[3]) if len(sys.argv) > 3 else 50

    optimize_counter_deck(
        cards_json,
        model_path=model_path,
        generations=generations,
        population_size=pop_size,
    )
