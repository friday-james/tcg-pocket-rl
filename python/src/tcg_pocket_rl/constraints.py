"""Deck constraint definitions for constrained deck optimization."""

from dataclasses import dataclass, field
from typing import Callable


@dataclass
class DeckConstraints:
    """Constraints for deck building.

    All constraints are optional. Cards must pass ALL specified constraints
    to be included in the available card pool.
    """

    # Card availability
    available_cards: set[str] | None = None  # Card slugs player owns
    required_cards: list[str] | None = None  # Must include these slugs
    excluded_cards: set[str] | None = None  # Cannot include these slugs

    # Type restrictions
    allowed_types: set[str] | None = None  # Energy types (fire, water, etc.)

    # Set restrictions
    allowed_sets: set[str] | None = None  # Only from these sets

    # Rarity restrictions
    max_rarity: str | None = None  # Budget constraint
    excluded_rarities: set[str] | None = None  # No cards of these rarities

    # Custom filter
    custom_filter: Callable | None = None  # card -> bool

    # Rarity ordering for max_rarity comparison
    RARITY_ORDER = {
        "Common": 0, "common": 0, "C": 0,
        "Uncommon": 1, "uncommon": 1, "U": 1,
        "Rare": 2, "rare": 2, "R": 2,
        "Double Rare": 3, "double rare": 3, "RR": 3,
        "Art Rare": 4, "art rare": 4, "AR": 4,
        "Shiny Rare": 4, "shiny rare": 4,
        "Special Art Rare": 5, "special art rare": 5, "SAR": 5, "SR": 5,
        "Double Shiny Rare": 5, "double shiny rare": 5,
        "Immersive Rare": 6, "immersive rare": 6, "IM": 6,
        "Crown Rare": 7, "crown rare": 7, "CR": 7,
        "Promo": 2, "promo": 2,
    }

    def filter_card_pool(self, all_cards: list[dict]) -> list[dict]:
        """Filter cards to only those meeting all constraints."""
        pool = []
        for card in all_cards:
            if not self._card_passes(card):
                continue
            pool.append(card)
        return pool

    def _card_passes(self, card: dict) -> bool:
        """Check if a single card passes all constraints."""
        slug = card.get("slug", "")

        if self.available_cards is not None and slug not in self.available_cards:
            return False

        if self.excluded_cards is not None and slug in self.excluded_cards:
            return False

        if self.allowed_types is not None and card.get("card_type") == "pokemon":
            energy = card.get("energy_type", "").lower()
            if energy and energy not in self.allowed_types:
                return False

        if self.allowed_sets is not None:
            card_set = card.get("set_name", "")
            if card_set not in self.allowed_sets:
                return False

        if self.max_rarity is not None:
            card_rarity = card.get("rarity", "")
            max_rank = self.RARITY_ORDER.get(self.max_rarity, 99)
            card_rank = self.RARITY_ORDER.get(card_rarity, 99)
            if card_rank > max_rank:
                return False

        if self.excluded_rarities is not None:
            card_rarity = card.get("rarity", "")
            if card_rarity in self.excluded_rarities:
                return False

        if self.custom_filter is not None:
            if not self.custom_filter(card):
                return False

        return True

    def validate_deck(self, deck_ids: list[str], all_cards: list[dict]) -> list[str]:
        """Validate a deck against constraints. Returns list of violations."""
        violations = []
        slug_to_card = {c["slug"]: c for c in all_cards}

        if len(deck_ids) != 20:
            violations.append(f"Deck has {len(deck_ids)} cards (need 20)")

        # Check required cards
        if self.required_cards:
            for slug in self.required_cards:
                if slug not in deck_ids:
                    name = slug_to_card.get(slug, {}).get("name", slug)
                    violations.append(f"Missing required card: {name}")

        # Check each card passes constraints
        for slug in deck_ids:
            card = slug_to_card.get(slug)
            if card and not self._card_passes(card):
                violations.append(f"Card not allowed: {card.get('name', slug)}")

        return violations
