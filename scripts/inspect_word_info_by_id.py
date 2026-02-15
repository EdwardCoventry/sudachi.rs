#!/usr/bin/env python3
"""Quick manual check for packed word-id -> WordInfo resolution in SudachiPy."""

from __future__ import annotations

import argparse
from pathlib import Path

from sudachipy import Dictionary


WORD_MASK = (1 << 28) - 1


def unpack(word_id: int) -> tuple[int, int]:
    return (word_id >> 28) & 0xF, word_id & WORD_MASK


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("surface", nargs="?", default="東京府")
    parser.add_argument(
        "--resource-dir",
        default=str(Path(__file__).resolve().parents[1] / "python" / "tests" / "resources"),
    )
    parser.add_argument(
        "--config",
        default=None,
        help="Path to Sudachi config json. Defaults to <resource-dir>/sudachi.json",
    )
    args = parser.parse_args()

    resource_dir = Path(args.resource_dir)
    config_path = Path(args.config) if args.config else (resource_dir / "sudachi.json")

    dic = Dictionary(str(config_path), resource_dir=str(resource_dir))
    out = dic.lookup(args.surface)

    print(f"surface={args.surface!r} matches={len(out)}")
    for i, m in enumerate(out):
        wid = m.word_id()
        did, lid = unpack(wid)
        wi = dic.word_info(wid)
        print(
            f"[{i}] token={m.surface()} word_id={wid} (dic={did}, lex={lid}) "
            f"wi.surface={wi.surface} wi.dictionary_id={wi.dictionary_id}"
        )

        if wi.a_unit_split:
            print("    A splits:")
            for swid in wi.a_unit_split:
                sdid, slid = unpack(swid)
                swi = dic.word_info(swid)
                print(
                    f"      - {swid} (dic={sdid}, lex={slid}) surface={swi.surface} "
                    f"dictionary_id={swi.dictionary_id}"
                )


if __name__ == "__main__":
    main()
