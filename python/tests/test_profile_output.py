"""Reusable output lists cannot replace the selected dictionary/profile owner."""

from pathlib import Path

import pytest
from sudachipy import Dictionary,SplitMode


def test_foreign_dictionary_output_is_rejected_without_erasing_results():
    resources=Path(__file__).parent/'resources'
    first=Dictionary(str(resources/'sudachi.json'),resource_dir=str(resources))
    second=Dictionary(str(resources/'sudachi.json'),resource_dir=str(resources))
    try:
        tokenizer=first.create()
        foreign=second.create().tokenize('京都')
        expected=[m.surface() for m in foreign]
        source=tokenizer.tokenize('東京都',mode=SplitMode.C)
        for operation in (
            lambda:tokenizer.tokenize('東京都',out=foreign),
            lambda:tokenizer.tokenize_forced_splits('東京 都',out=foreign),
            lambda:first.lookup('東京都',out=foreign),
            lambda:source[0].split(SplitMode.A,out=foreign),
        ):
            with pytest.raises(ValueError,match='different dictionary/profile'):
                operation()
            assert [m.surface() for m in foreign]==expected
    finally:
        first.close()
        second.close()
