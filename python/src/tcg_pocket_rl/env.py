"""Gymnasium environment wrapping the Rust game engine."""

import numpy as np
import gymnasium as gym
from gymnasium import spaces

from tcg_pocket_engine import PyGameEngine, OBS_SIZE, ACTION_SPACE_SIZE


class PokemonTCGPocketEnv(gym.Env):
    """Pokemon TCG Pocket environment for reinforcement learning.

    Wraps the Rust game engine as a Gymnasium environment with action masking
    support for MaskablePPO (sb3-contrib).
    """

    metadata = {"render_modes": ["ansi"]}

    def __init__(
        self,
        cards_json: str,
        deck1_ids: list[str] | None = None,
        deck2_ids: list[str] | None = None,
        opponent_policy=None,
        agent_player: int = 0,
        render_mode: str | None = None,
    ):
        super().__init__()

        self.engine = PyGameEngine(cards_json)
        self.deck1_ids = deck1_ids
        self.deck2_ids = deck2_ids
        self.opponent_policy = opponent_policy
        self.agent_player = agent_player
        self.render_mode = render_mode
        self._seed = 0

        self.observation_space = spaces.Box(
            low=0.0, high=1.0, shape=(OBS_SIZE,), dtype=np.float32
        )
        self.action_space = spaces.Discrete(ACTION_SPACE_SIZE)

    def reset(self, seed=None, options=None):
        super().reset(seed=seed)
        if seed is not None:
            self._seed = seed
        else:
            self._seed += 1

        if self.deck1_ids is None or self.deck2_ids is None:
            raise ValueError("deck1_ids and deck2_ids must be provided")

        self.engine.reset(
            self.deck1_ids,
            self.deck2_ids,
            seed=self._seed,
            agent_player=self.agent_player,
        )

        # Play opponent turns during setup if they go first
        self._play_opponent_turns()

        obs = np.array(self.engine.observation(), dtype=np.float32)
        info = {"action_mask": np.array(self.action_masks(), dtype=np.bool_)}
        return obs, info

    def step(self, action):
        reward, done = self._safe_step(int(action))

        if not done:
            # Play opponent turns until it's the agent's turn again
            opp_result = self._play_opponent_turns()
            if self.engine.is_done():
                done = True
                reward = opp_result

        obs = np.array(self.engine.observation(), dtype=np.float32)
        info = {"action_mask": np.array(self.action_masks(), dtype=np.bool_)}
        return obs, reward, done, False, info

    def _safe_step(self, action: int) -> tuple[float, bool]:
        """Execute a step with fallback on invalid action."""
        try:
            _, reward, done, _, _ = self.engine.step(action)
            return reward, done
        except ValueError:
            # Action was masked as legal but engine rejected it - recover
            legal = self.engine.legal_action_indices()
            if legal:
                _, reward, done, _, _ = self.engine.step(legal[0])
                return reward, done
            # No legal actions - force end turn
            try:
                _, reward, done, _, _ = self.engine.step(114)
                return reward, done
            except ValueError:
                # Game is in a broken state - treat as done
                return 0.0, True

    def action_masks(self) -> np.ndarray:
        """Return action mask for MaskablePPO compatibility.

        Never returns all-zeros: MaskableCategorical requires at least one
        valid action to satisfy the Simplex constraint during policy updates.
        """
        if self.engine.is_done():
            mask = np.zeros(ACTION_SPACE_SIZE, dtype=np.bool_)
            mask[0] = True  # dummy valid action for terminal state
            return mask
        mask = np.array(self.engine.action_masks(), dtype=np.bool_)
        if not mask.any():
            mask[0] = True  # safety: ensure at least one valid action
        return mask

    def render(self):
        if self.render_mode == "ansi":
            return self.engine.render()
        return None

    def _play_opponent_turns(self) -> float:
        """Play opponent turns using the opponent policy.

        Returns the agent's reward if the game ended during opponent play.
        """
        for _ in range(500):  # safety limit
            if self.engine.is_done():
                # Game ended - determine reward
                return 1.0 if self.engine.current_player() != self.agent_player else -1.0

            if self.engine.current_player() == self.agent_player:
                break

            legal = self.engine.legal_action_indices()
            if not legal:
                break

            if self.opponent_policy is not None:
                action = self.opponent_policy(self.engine, legal)
            else:
                action = legal[np.random.randint(len(legal))]

            reward, done = self._safe_step(action)
            if done:
                return -reward  # flip: opponent's positive = agent's negative

        return 0.0
