use pyo3::prelude::*;
use pyo3::exceptions::PyValueError;
use std::path::Path;

use crate::bridge::action_map::{action_mask, action_to_index, index_to_action, ACTION_SPACE_SIZE};
use crate::bridge::observation::{encode_observation, OBS_SIZE};
use crate::data::deck::Deck;
use crate::data::loader::{load_card_database, CardDatabase};
use crate::game::actions::legal_actions;
use crate::game::engine::{apply_action, new_game, StepResult};
use crate::game::rng::GameRng;
use crate::game::state::GameState;

/// Python-facing game engine that manages the full game loop.
#[pyclass]
pub struct PyGameEngine {
    db: CardDatabase,
    state: Option<GameState>,
    rng: Option<GameRng>,
    /// Which player the agent controls (0 or 1).
    agent_player: usize,
}

#[pymethods]
impl PyGameEngine {
    /// Create a new engine, loading the card database from a JSON file.
    #[new]
    fn new(cards_json_path: &str) -> PyResult<Self> {
        let db = load_card_database(Path::new(cards_json_path))
            .map_err(|e| PyValueError::new_err(e))?;
        Ok(PyGameEngine {
            db,
            state: None,
            rng: None,
            agent_player: 0,
        })
    }

    /// Reset the environment with two decks (lists of card IDs/slugs).
    /// Returns the initial observation vector.
    #[pyo3(signature = (deck1_ids, deck2_ids, seed=42, agent_player=0))]
    fn reset(
        &mut self,
        deck1_ids: Vec<String>,
        deck2_ids: Vec<String>,
        seed: u64,
        agent_player: usize,
    ) -> PyResult<Vec<f32>> {
        let deck1 = self.build_deck(&deck1_ids)?;
        let deck2 = self.build_deck(&deck2_ids)?;

        let (state, rng) = new_game(deck1, deck2, seed);
        self.state = Some(state);
        self.rng = Some(rng);
        self.agent_player = agent_player;

        Ok(self.get_observation())
    }

    /// Take an action (by index) and return (obs, reward, done, truncated, info_dict).
    fn step(&mut self, action_idx: usize) -> PyResult<(Vec<f32>, f32, bool, bool, String)> {
        let action = index_to_action(action_idx)
            .ok_or_else(|| PyValueError::new_err(format!("Invalid action index: {}", action_idx)))?;

        let (reward, done) = {
            let state = self.state.as_mut()
                .ok_or_else(|| PyValueError::new_err("Game not initialized. Call reset() first."))?;
            let rng = self.rng.as_mut().unwrap();

            let result = apply_action(state, &action, rng);

            match result {
                StepResult::Continue => (0.0, false),
                StepResult::GameOver { winner } => {
                    let r = if winner == self.agent_player { 1.0 } else { -1.0 };
                    (r, true)
                }
                StepResult::InvalidAction(msg) => {
                    return Err(PyValueError::new_err(format!("Invalid action: {}", msg)));
                }
            }
        };

        // Check terminal state and compute final reward
        let state = self.state.as_ref().unwrap();
        let done = done || state.is_terminal();
        let final_reward = if done && reward == 0.0 {
            if let Some(w) = state.winner {
                if w == self.agent_player { 1.0 } else { -1.0 }
            } else {
                0.0
            }
        } else {
            reward
        };

        let info = format!(
            "{{\"turn\": {}, \"phase\": \"{:?}\", \"current_player\": {}}}",
            state.turn_number, state.phase, state.current_player
        );

        let obs = self.get_observation();
        Ok((obs, final_reward, done, false, info))
    }

    /// Get the legal action mask (bool vector of size ACTION_SPACE_SIZE).
    fn action_masks(&self) -> PyResult<Vec<bool>> {
        let state = self.state.as_ref()
            .ok_or_else(|| PyValueError::new_err("Game not initialized"))?;
        Ok(action_mask(state))
    }

    /// Get legal action indices.
    fn legal_action_indices(&self) -> PyResult<Vec<usize>> {
        let state = self.state.as_ref()
            .ok_or_else(|| PyValueError::new_err("Game not initialized"))?;
        let actions = legal_actions(state);
        Ok(actions.iter().map(action_to_index).collect())
    }

    /// Get the observation size.
    #[staticmethod]
    fn obs_size() -> usize {
        OBS_SIZE
    }

    /// Get the action space size.
    #[staticmethod]
    fn action_space_size() -> usize {
        ACTION_SPACE_SIZE
    }

    /// Get current player index.
    fn current_player(&self) -> PyResult<usize> {
        let state = self.state.as_ref()
            .ok_or_else(|| PyValueError::new_err("Game not initialized"))?;
        Ok(state.current_player)
    }

    /// Whether the game is over.
    fn is_done(&self) -> PyResult<bool> {
        let state = self.state.as_ref()
            .ok_or_else(|| PyValueError::new_err("Game not initialized"))?;
        Ok(state.is_terminal())
    }

    /// Get a text rendering of the board state.
    fn render(&self) -> PyResult<String> {
        let state = self.state.as_ref()
            .ok_or_else(|| PyValueError::new_err("Game not initialized"))?;
        Ok(render_state(state))
    }

    /// Get the current observation vector for the agent.
    fn observation(&self) -> PyResult<Vec<f32>> {
        self.state.as_ref()
            .ok_or_else(|| PyValueError::new_err("Game not initialized"))
            .map(|s| encode_observation(s, self.agent_player))
    }

    /// Get number of cards in the database.
    fn num_cards(&self) -> usize {
        self.db.cards.len()
    }

    /// Get all card IDs (slugs) in the database.
    fn card_ids(&self) -> Vec<String> {
        self.db.cards.iter().map(|c| c.id.clone()).collect()
    }

    /// Get card names.
    fn card_names(&self) -> Vec<String> {
        self.db.cards.iter().map(|c| c.name.clone()).collect()
    }

    /// Look up a card by ID and return its info as a dict-like string.
    fn card_info(&self, card_id: &str) -> PyResult<String> {
        let card = self.db.get_by_id(card_id)
            .ok_or_else(|| PyValueError::new_err(format!("Card not found: {}", card_id)))?;
        Ok(format!(
            "{{\"name\": \"{}\", \"type\": \"{:?}\", \"hp\": {:?}, \"attacks\": {}}}",
            card.name,
            card.card_type,
            card.hp,
            card.attacks.len()
        ))
    }
}

impl PyGameEngine {
    fn build_deck(&self, card_ids: &[String]) -> PyResult<Deck> {
        let mut cards = Vec::new();
        for id in card_ids {
            let card = self.db.get_by_id(id)
                .or_else(|| self.db.get_by_name(id))
                .ok_or_else(|| PyValueError::new_err(format!("Card not found: {}", id)))?;
            cards.push(card.clone());
        }
        Deck::new(cards).map_err(|e| PyValueError::new_err(e.to_string()))
    }

    fn get_observation(&self) -> Vec<f32> {
        match &self.state {
            Some(state) => encode_observation(state, self.agent_player),
            None => vec![0.0; OBS_SIZE],
        }
    }
}

/// Render a game state as readable text.
fn render_state(state: &GameState) -> String {
    let mut s = String::new();
    s.push_str(&format!("Turn {} | Phase: {:?} | Player {}'s turn\n",
        state.turn_number, state.phase, state.current_player));
    s.push_str(&format!("Score: P0={} P1={}\n\n", state.players[0].points, state.players[1].points));

    for p in 0..2 {
        s.push_str(&format!("=== Player {} ===\n", p));
        s.push_str(&format!("Hand: {} cards | Deck: {} | Discard: {}\n",
            state.players[p].hand.len(),
            state.players[p].deck.len(),
            state.players[p].discard.len()));

        if let Some(ref active) = state.players[p].active {
            s.push_str(&format!("Active: {} ({}/{}HP) Energy: {} Status: {:?}\n",
                active.card.name,
                active.remaining_hp(),
                active.max_hp(),
                active.attached_energy.len(),
                active.status_conditions));
        } else {
            s.push_str("Active: (none)\n");
        }

        for (i, slot) in state.players[p].bench.iter().enumerate() {
            if let Some(ref pokemon) = slot {
                s.push_str(&format!("Bench {}: {} ({}/{}HP) Energy: {}\n",
                    i, pokemon.card.name,
                    pokemon.remaining_hp(),
                    pokemon.max_hp(),
                    pokemon.attached_energy.len()));
            }
        }
        s.push('\n');
    }

    if state.is_terminal() {
        if let Some(winner) = state.winner {
            s.push_str(&format!("GAME OVER: Player {} wins!\n", winner));
        }
    }

    s
}

/// Register the PyO3 module.
#[pymodule]
pub fn tcg_pocket_engine(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyGameEngine>()?;
    m.add("OBS_SIZE", OBS_SIZE)?;
    m.add("ACTION_SPACE_SIZE", ACTION_SPACE_SIZE)?;
    Ok(())
}
