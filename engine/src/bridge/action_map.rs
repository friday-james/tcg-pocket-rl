use crate::data::card::EnergyType;
use crate::game::actions::Action;
use crate::game::state::GameState;

/// Total number of discrete action indices.
pub const ACTION_SPACE_SIZE: usize = 512;

// Action space layout:
// [0-9]     PlaceActive(0..9)           - hand index for setup
// [10-19]   PlaceBench(0..9)            - hand index for setup
// [20]      ConfirmSetup
// [21-30]   PlayPokemonToBench(0..9)    - hand index
// [31-70]   EvolvePokemon(hand, board)  - 10 hand * 4 board positions
// [71-79]   SetEnergyZoneType(0..8)     - 9 concrete energy types
// [80-83]   AttachEnergy(0..3)          - board positions
// [84-86]   Retreat(0..2)               - bench indices
// [87-90]   UseAbility(0..3)            - board positions
// [91-100]  PlayTrainer(0..9)           - hand index
// [101-110] PlaySupporter(0..9)         - hand index
// [111-113] UseAttack(0..2)             - attack index
// [114]     EndTurn
// [115-118] ChooseTarget(0..3)          - board positions
// [119-128] ChooseOption(0..9)          - option index
// [129-131] PromotePokemon(0..2)        - bench index
// [132-511] Reserved

/// Convert an Action to a discrete index.
pub fn action_to_index(action: &Action) -> usize {
    match action {
        Action::PlaceActive(i) => *i,
        Action::PlaceBench(i) => 10 + i,
        Action::ConfirmSetup => 20,
        Action::PlayPokemonToBench(i) => 21 + i,
        Action::EvolvePokemon(hand, board) => 31 + hand * 4 + board,
        Action::SetEnergyZoneType(et) => 71 + energy_type_to_idx(*et),
        Action::AttachEnergy(pos) => 80 + pos,
        Action::Retreat(bench) => 84 + bench,
        Action::UseAbility(pos) => 87 + pos,
        Action::PlayTrainer(i) => 91 + i,
        Action::PlaySupporter(i) => 101 + i,
        Action::UseAttack(i) => 111 + i,
        Action::EndTurn => 114,
        Action::ChooseTarget(pos) => 115 + pos,
        Action::ChooseOption(i) => 119 + i,
        Action::PromotePokemon(bench) => 129 + bench,
    }
}

/// Convert a discrete index back to an Action.
pub fn index_to_action(idx: usize) -> Option<Action> {
    match idx {
        0..=9 => Some(Action::PlaceActive(idx)),
        10..=19 => Some(Action::PlaceBench(idx - 10)),
        20 => Some(Action::ConfirmSetup),
        21..=30 => Some(Action::PlayPokemonToBench(idx - 21)),
        31..=70 => {
            let offset = idx - 31;
            Some(Action::EvolvePokemon(offset / 4, offset % 4))
        }
        71..=79 => idx_to_energy_type(idx - 71).map(Action::SetEnergyZoneType),
        80..=83 => Some(Action::AttachEnergy(idx - 80)),
        84..=86 => Some(Action::Retreat(idx - 84)),
        87..=90 => Some(Action::UseAbility(idx - 87)),
        91..=100 => Some(Action::PlayTrainer(idx - 91)),
        101..=110 => Some(Action::PlaySupporter(idx - 101)),
        111..=113 => Some(Action::UseAttack(idx - 111)),
        114 => Some(Action::EndTurn),
        115..=118 => Some(Action::ChooseTarget(idx - 115)),
        119..=128 => Some(Action::ChooseOption(idx - 119)),
        129..=131 => Some(Action::PromotePokemon(idx - 129)),
        _ => None,
    }
}

/// Generate action mask for the current game state.
pub fn action_mask(state: &GameState) -> Vec<bool> {
    let legal = crate::game::actions::legal_actions(state);
    let mut mask = vec![false; ACTION_SPACE_SIZE];
    for action in &legal {
        let idx = action_to_index(action);
        if idx < ACTION_SPACE_SIZE {
            mask[idx] = true;
        }
    }
    mask
}

fn energy_type_to_idx(et: EnergyType) -> usize {
    match et {
        EnergyType::Grass => 0,
        EnergyType::Fire => 1,
        EnergyType::Water => 2,
        EnergyType::Lightning => 3,
        EnergyType::Psychic => 4,
        EnergyType::Fighting => 5,
        EnergyType::Darkness => 6,
        EnergyType::Metal => 7,
        EnergyType::Dragon => 8,
        EnergyType::Colorless => 9,
    }
}

fn idx_to_energy_type(idx: usize) -> Option<EnergyType> {
    match idx {
        0 => Some(EnergyType::Grass),
        1 => Some(EnergyType::Fire),
        2 => Some(EnergyType::Water),
        3 => Some(EnergyType::Lightning),
        4 => Some(EnergyType::Psychic),
        5 => Some(EnergyType::Fighting),
        6 => Some(EnergyType::Darkness),
        7 => Some(EnergyType::Metal),
        8 => Some(EnergyType::Dragon),
        _ => None,
    }
}
