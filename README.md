# Pokemon TCG Pocket RL

Reinforcement learning agent for Pokemon TCG Pocket, built with a Rust game engine and trained using MaskablePPO.

## Architecture

```
data/           Card database (scraped from pokemontcgpocket.app)
engine/         Rust game engine with full game logic
  src/game/     State, actions, engine loop
  src/effects/  Card effect registry + executor
  src/bridge/   PyO3 Python bindings
python/         Gymnasium environment wrapper
scripts/        Training and evaluation scripts
checkpoints/    Saved model weights
```

## Stack

- **Game Engine**: Rust with PyO3 bindings (~512 action space, normalized observation vectors)
- **RL Framework**: MaskablePPO from sb3-contrib (action masking for legal moves)
- **Training**: 2M timesteps across 132 meta deck matchup pairs, ~275 fps on CPU

## Agent vs Agent Results

Trained model plays both sides with correct per-player observations. 50 games per matchup, 132 matchups (6,600 total games).

### Tier Ranking

| Rank | Tier | Deck | Win Rate |
|------|------|------|----------|
| 1 | **A** | **Dialga Palkia** | **52.9%** |
| 2 | B | Pikachu ex Aggro | 51.6% |
| 3 | B | Starmie ex | 51.1% |
| 4 | B | Buzzwole Lucario | 50.7% |
| 5 | B | Suicune Greninja | 50.4% |
| 6 | B | Mega Altaria | 50.2% |
| 7 | B | Gyarados ex | 49.6% |
| 8 | B | Celebi Exeggutor | 49.3% |
| 9 | B | Mew Mewtwo | 48.4% |
| 10 | B | Charizard ex | 48.2% |
| 11 | C | Mewtwo ex Control | 47.8% |
| 12 | C | Darkrai Hydreigon | 47.5% |

### Best Deck: Dialga Palkia (Water)

```
2x Dialga ex          2x Palkia ex
2x Suicune ex         2x Articuno ex
2x Mantyke            2x Misty
2x Sabrina            2x Professor's Research
2x Poke Ball          2x Giovanni
```

### Matchup Matrix

Win counts out of 50 games (row vs column):

|  | Pika | Mew2 | Suic | Char | Cele | Star | Dark | Dial | Buzz | MewM | Gyar | Mega |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| **Pikachu ex** | - | 26 | 27 | 29 | 25 | 24 | 22 | 25 | 27 | 26 | 28 | 25 |
| **Mewtwo ex** | 24 | - | 24 | 26 | 24 | 29 | 23 | 25 | 23 | 24 | 21 | 20 |
| **Suicune** | 25 | 25 | - | 27 | 24 | 25 | 26 | 24 | 25 | 28 | 27 | 21 |
| **Charizard** | 23 | 28 | 23 | - | 25 | 24 | 26 | 26 | 19 | 25 | 26 | 20 |
| **Celebi** | 26 | 27 | 27 | 21 | - | 25 | 26 | 25 | 24 | 25 | 24 | 21 |
| **Starmie** | 27 | 27 | 22 | 21 | 25 | - | 26 | 25 | 30 | 24 | 26 | 28 |
| **Darkrai** | 24 | 26 | 23 | 23 | 22 | 24 | - | 20 | 24 | 25 | 25 | 25 |
| **Dialga** | 28 | 28 | 26 | 24 | 25 | 27 | 25 | - | 26 | 27 | 29 | 26 |
| **Buzzwole** | 23 | 25 | 22 | 29 | 26 | 25 | 27 | 23 | - | 25 | 26 | 28 |
| **Mew Mew2** | 26 | 25 | 25 | 27 | 25 | 25 | 25 | 21 | 19 | - | 24 | 24 |
| **Gyarados** | 26 | 28 | 25 | 21 | 27 | 26 | 26 | 24 | 24 | 27 | - | 19 |
| **Mega Altaria** | 25 | 23 | 26 | 23 | 28 | 24 | 24 | 24 | 27 | 25 | 27 | - |

### Key Matchup Insights

- **Dialga Palkia** is the only A-tier deck, with positive matchups across the board
- **Starmie ex** hard-counters Buzzwole Lucario (60% win rate)
- **Buzzwole Lucario** hard-counters Charizard ex (58%)
- **Pikachu ex Aggro** dominates Charizard ex (58%) and Gyarados ex (56%)
- The meta is well-balanced: all decks fall within 47-53%, no dominant outlier

## Optimized Counter-Deck: Fighting Blissey ex (68.3%)

A genetic algorithm evolved this deck over 30 generations to beat all 12 meta decks. Fitness is measured by agent-vs-agent win rate (trained model plays both sides). Energy Zone constraint enforced: all attack costs use Fighting + Colorless only.

### Decklist (Energy Zone: Fighting)

**Pokemon (8):**

| Qty | Card | HP | Attack | Cost | Dmg | Effect |
|-----|------|----|--------|------|-----|--------|
| 1x | Blissey ex | 180 | Happy Punch | CCCC | 100 | Flip heads: heal 60 |
| 1x | Larvitar | 60 | Corkscrew Punch | FC | 30 | — |
| 1x | Pupitar | 80 | Guard Press | CCC | 20 | -30 dmg next turn |
| 1x | Geodude | 70 | Tackle | F | 20 | — |
| 1x | Farfetch'd | 60 | Leek Slap | C | 40 | — |
| 1x | Eevee | 60 | Tail Whap | CC | 30 | — |
| 1x | Leafeon | 90 | Leaf Blast | C | 10+ | +20 per Grass Energy |
| 1x | Pineco | 60 | Ram | CC | 30 | — |

**Trainers (12):**

| Qty | Card | Type | Effect |
|-----|------|------|--------|
| 1x | Iono | Supporter | Both players shuffle hand, draw same count |
| 1x | Mars | Supporter | Opponent draws cards = remaining prize points |
| 1x | Leaf | Supporter | Retreat cost -2 this turn |
| 1x | Celestic Town Elder | Supporter | Recover random Basic from discard |
| 1x | Pokemon Flute | Item | Put Basic from opponent's discard onto their bench |
| 1x | Eevee Bag | Item | Eevee evo +10 dmg or heal 20 |
| 1x | Elemental Switch | Item | Move Fire/Water Energy bench to active |
| 1x | Sitrus Berry | Tool | Heal 30 at half HP, discard |
| 1x | Lusamine | Supporter | Attach 2 Energy from discard to Ultra Beast |
| 1x | Mallow | Supporter | Full heal Shiinotic/Tsareena, discard energy |
| 1x | Kiawe | Supporter | Attach 2 Fire Energy to Marowak/Turtonator |
| 1x | Team Rocket Grunt | Supporter | Disruption |

### Matchups vs Meta

| Opponent | Win Rate |
|----------|----------|
| Mew Mewtwo | **100%** |
| Mewtwo ex Control | **80%** |
| Buzzwole Lucario | **80%** |
| Suicune Greninja | 70% |
| Starmie ex | 70% |
| Darkrai Hydreigon | 70% |
| Dialga Palkia | 70% |
| Charizard ex | 60% |
| Celebi Exeggutor | 60% |
| Gyarados ex | 60% |
| Pikachu ex Aggro | 50% |
| Mega Altaria | 50% |

No losing matchups. The deck's core strategy: **Blissey ex** (180HP, 100 dmg + coin-flip heal) as a tanky primary attacker, backed by disruption supporters and cheap colorless attackers for early pressure.

## Training

```bash
# Install dependencies
pip install sb3-contrib stable-baselines3 gymnasium

# Build engine
cd engine && maturin develop --release --features python

# Train (2M steps, 8 parallel envs)
python scripts/train_meta.py 2000000 8

# Resume from checkpoint
python scripts/train_meta.py 2000000 8 checkpoints/ppo_meta_655360

# Agent-vs-agent evaluation
python scripts/eval_agent_vs_agent.py checkpoints/ppo_meta_final 50

# Evolve a counter-deck (30 generations, population 50)
python scripts/optimize_counter_deck.py checkpoints/ppo_meta_final 30 50
```

## Meta Decks (February 2026)

All 12 decks are defined in `scripts/train_meta.py` with 20 cards each (2 copies per card). Sourced from game8.co, pokemontcgpocket.app, ptcgpocket.gg, and pokemonmeta.com tier lists.
