use super::card::{Card, Stage};
use std::collections::HashMap;

/// Maximum number of cards in a Pokemon TCG Pocket deck.
pub const DECK_SIZE: usize = 20;

/// Maximum copies of any single card (by name) in a deck.
pub const MAX_COPIES: usize = 2;

/// A deck of 20 cards.
#[derive(Debug, Clone)]
pub struct Deck {
    pub cards: Vec<Card>,
}

#[derive(Debug)]
pub enum DeckError {
    WrongSize { actual: usize },
    TooManyCopies { name: String, count: usize },
    NoBasicPokemon,
    BrokenEvolutionLine { name: String, missing: String },
}

impl std::fmt::Display for DeckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeckError::WrongSize { actual } => {
                write!(f, "Deck has {} cards, expected {}", actual, DECK_SIZE)
            }
            DeckError::TooManyCopies { name, count } => {
                write!(f, "Too many copies of '{}': {} (max {})", name, count, MAX_COPIES)
            }
            DeckError::NoBasicPokemon => write!(f, "Deck has no basic Pokemon"),
            DeckError::BrokenEvolutionLine { name, missing } => {
                write!(f, "'{}' needs '{}' but it's not in the deck", name, missing)
            }
        }
    }
}

impl Deck {
    pub fn new(cards: Vec<Card>) -> Result<Self, DeckError> {
        let deck = Deck { cards };
        deck.validate()?;
        Ok(deck)
    }

    /// Create a deck without validation (for testing / optimization).
    pub fn new_unchecked(cards: Vec<Card>) -> Self {
        Deck { cards }
    }

    pub fn validate(&self) -> Result<(), DeckError> {
        // Check size
        if self.cards.len() != DECK_SIZE {
            return Err(DeckError::WrongSize {
                actual: self.cards.len(),
            });
        }

        // Check max copies
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for card in &self.cards {
            let count = counts.entry(&card.name).or_insert(0);
            *count += 1;
            if *count > MAX_COPIES {
                return Err(DeckError::TooManyCopies {
                    name: card.name.clone(),
                    count: *count,
                });
            }
        }

        // Check at least one basic Pokemon
        if !self.cards.iter().any(|c| c.is_basic_pokemon()) {
            return Err(DeckError::NoBasicPokemon);
        }

        // Check evolution lines: every Stage 1/2 must have its pre-evolution in the deck
        let names_in_deck: std::collections::HashSet<&str> =
            self.cards.iter().map(|c| c.name.as_str()).collect();

        for card in &self.cards {
            if let Some(ref evolves_from) = card.evolves_from {
                if !names_in_deck.contains(evolves_from.as_str()) {
                    return Err(DeckError::BrokenEvolutionLine {
                        name: card.name.clone(),
                        missing: evolves_from.clone(),
                    });
                }
            }
        }

        Ok(())
    }

    /// Count how many basic Pokemon are in this deck.
    pub fn basic_pokemon_count(&self) -> usize {
        self.cards.iter().filter(|c| c.is_basic_pokemon()).count()
    }

    /// Count how many trainer cards are in this deck.
    pub fn trainer_count(&self) -> usize {
        self.cards.iter().filter(|c| c.is_trainer()).count()
    }

    /// Get all unique evolution lines in this deck.
    pub fn evolution_lines(&self) -> Vec<Vec<&Card>> {
        let mut lines: Vec<Vec<&Card>> = Vec::new();

        // Find all basic Pokemon
        let basics: Vec<&Card> = self.cards.iter().filter(|c| c.is_basic_pokemon()).collect();

        for basic in basics {
            let mut line = vec![basic];

            // Find Stage 1 that evolves from this basic
            for card in &self.cards {
                if card.stage == Some(Stage::Stage1)
                    && card.evolves_from.as_deref() == Some(&basic.name)
                {
                    line.push(card);

                    // Find Stage 2 that evolves from this Stage 1
                    for card2 in &self.cards {
                        if card2.stage == Some(Stage::Stage2)
                            && card2.evolves_from.as_deref() == Some(&card.name)
                        {
                            line.push(card2);
                            break;
                        }
                    }
                    break;
                }
            }

            lines.push(line);
        }

        lines
    }
}
