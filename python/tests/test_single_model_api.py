"""The native constructor exposes only the single-source profile API."""
import pytest

from sudachipy import Dictionary


def test_retired_hybrid_scoring_option_is_rejected():
    with pytest.raises(TypeError, match='scoring_profile'):
        Dictionary(scoring_profile='retired-hybrid.json')


def test_native_constructor_cannot_take_a_list_of_main_models():
    with pytest.raises(TypeError):
        Dictionary(tokenisation_profile=['spoken.json', 'waka.json'])
