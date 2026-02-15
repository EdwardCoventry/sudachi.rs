# Copyright (c) 2026 Works Applications Co., Ltd.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from sudachipy import Dictionary


OUTPUT_DIR = Path(__file__).parent / "output"
OUTPUT_JSON = OUTPUT_DIR / "reading_candidates_diagnostics.json"
OUTPUT_MD = OUTPUT_DIR / "reading_candidates_diagnostics.md"


CASES: list[dict[str, Any]] = [
    {
        "id": "basic_kanji_kana",
        "text": "東京都。",
        "reading": "トウキョウト。",
        "note": "Baseline kanji + punctuation.",
        "expected_min": 1,
        "min_tokens": 1,
    },
    {
        "id": "basic_kanji_kana_min2",
        "text": "東京都",
        "reading": "トウキョウト",
        "note": "Require at least two tokens; single-token path should be excluded.",
        "expected_min": 1,
        "min_tokens": 2,
    },
    {
        "id": "slashes_and_latin",
        "text": "A/B",
        "reading": "A/B",
        "note": "Uppercase latin and symbol surface style.",
        "expected_min": 1,
    },
    {
        "id": "slashes_lowercase_surface",
        "text": "A/B",
        "reading": "a/b",
        "note": "Lowercase latin and symbol surface style.",
        "expected_min": 1,
    },
    {
        "id": "slashes_lowercase_symbol_reading",
        "text": "A/B",
        "reading": "aキゴウb",
        "note": "Lowercase latin and symbol reading-form style.",
        "expected_min": 1,
    },
    {
        "id": "slashes_fullwidth_surface",
        "text": "A/B",
        "reading": "ａ／ｂ",
        "note": "Full-width latin/symbol surface style.",
        "expected_min": 1,
    },
    {
        "id": "tilde_symbol",
        "text": "〜テスト",
        "reading": "〜テスト",
        "note": "Leading symbol kept as surface in reading string.",
        "expected_min": 1,
    },
    {
        "id": "tilde_symbol_word_reading",
        "text": "〜テスト",
        "reading": "キゴウテスト",
        "note": "Leading symbol read as word 'キゴウ'.",
        "expected_min": 1,
    },
    {
        "id": "digit_surface_style",
        "text": "第3話",
        "reading": "ダイ3ワ",
        "note": "Digit kept in reading string.",
        "expected_min": 1,
    },
    {
        "id": "digit_katakana_style",
        "text": "第3話",
        "reading": "ダイサンワ",
        "note": "Digit converted to katakana reading.",
        "expected_min": 1,
    },
    {
        "id": "mixed_number",
        "text": "123円",
        "reading": "123エン",
        "note": "Number surface style with suffix.",
        "expected_min": 1,
    },
    {
        "id": "mixed_number_reading_style",
        "text": "123円",
        "reading": "イチニサンエン",
        "note": "Number spoken reading style.",
        "expected_min": 1,
    },
    {
        "id": "hiragana_reading",
        "text": "東京都",
        "reading": "とうきょうと",
        "note": "Hiragana reading input.",
        "expected_min": 1,
    },
    {
        "id": "halfwidth_katakana_query",
        "text": "テスト",
        "reading": "ﾃｽﾄ",
        "note": "Half-width katakana reading input.",
        "expected_min": 1,
    },
    {
        "id": "ascii_punctuation_pair",
        "text": "！？",
        "reading": "!?",
        "note": "ASCII punctuation reading against full-width punctuation text.",
        "expected_min": 1,
    },
]


def _token_dict(m: Any) -> dict[str, Any]:
    pos = list(m.part_of_speech())
    return {
        "surface": m.surface(),
        "reading_form": m.reading_form(),
        "pos": pos,
        "dictionary_id": m.dictionary_id(),
        "word_id": m.word_id(),
    }


def _concat_reading(tokens: list[dict[str, Any]]) -> str:
    return "".join(t["reading_form"] for t in tokens)


def _concat_symbol_surface(tokens: list[dict[str, Any]]) -> str:
    parts: list[str] = []
    for t in tokens:
        if t["pos"][0] == "補助記号":
            parts.append(t["surface"])
        else:
            parts.append(t["reading_form"])
    return "".join(parts)


def _concat_symbol_number_surface(tokens: list[dict[str, Any]]) -> str:
    parts: list[str] = []
    for t in tokens:
        is_symbol = t["pos"][0] == "補助記号"
        is_number = t["pos"][0] == "名詞" and t["pos"][1] == "数詞"
        if is_symbol or is_number:
            parts.append(t["surface"])
        else:
            parts.append(t["reading_form"])
    return "".join(parts)


def build_report() -> dict[str, Any]:
    d = Dictionary()
    tok = d.create()
    rows: list[dict[str, Any]] = []
    try:
        for case in CASES:
            text = case["text"]
            reading = case["reading"]
            min_tokens = int(case.get("min_tokens", 1))

            tokenized = [_token_dict(m) for m in tok.tokenize(text)]
            candidates = tok.tokenize_reading_candidates(
                text, reading, max_results=25, min_tokens=min_tokens
            )
            expected_min = int(case.get("expected_min", 1))
            status = "ok" if len(candidates) >= expected_min else "needs_fix"

            row = {
                **case,
                "tokenized": tokenized,
                "derived_reading_form_concat": _concat_reading(tokenized),
                "derived_symbol_surface_concat": _concat_symbol_surface(tokenized),
                "derived_symbol_number_surface_concat": _concat_symbol_number_surface(tokenized),
                "min_tokens": min_tokens,
                "expected_min": expected_min,
                "status": status,
                "candidate_count": len(candidates),
                "top_candidates": candidates[:5],
            }
            rows.append(row)
    finally:
        d.close()

    return {"cases": rows}


def write_markdown(report: dict[str, Any]) -> str:
    lines: list[str] = []
    lines.append("# Reading Candidates Diagnostics")
    lines.append("")
    lines.append("| Case | Candidate Count | Text | Reading |")
    lines.append("|---|---:|---|---|")
    for row in report["cases"]:
        lines.append(
            f"| {row['id']} | {row['candidate_count']} | {row['text']} | {row['reading']} |"
        )
    lines.append("")

    for row in report["cases"]:
        lines.append(f"## {row['id']}")
        lines.append("")
        lines.append(f"- note: {row['note']}")
        lines.append(f"- text: `{row['text']}`")
        lines.append(f"- reading: `{row['reading']}`")
        lines.append(f"- min_tokens: {row['min_tokens']}")
        lines.append(f"- derived_reading_form_concat: `{row['derived_reading_form_concat']}`")
        lines.append(f"- derived_symbol_surface_concat: `{row['derived_symbol_surface_concat']}`")
        lines.append(
            f"- derived_symbol_number_surface_concat: `{row['derived_symbol_number_surface_concat']}`"
        )
        lines.append(f"- expected_min: {row['expected_min']}")
        lines.append(f"- candidate_count: {row['candidate_count']}")
        lines.append(f"- status: {row['status']}")
        if row["top_candidates"]:
            lines.append("- top_candidate_surfaces: " + " / ".join(
                ["+".join(t["surface"] for t in c["tokens"]) for c in row["top_candidates"][:3]]
            ))
        else:
            lines.append("- top_candidate_surfaces: (none)")
        lines.append("")

    return "\n".join(lines) + "\n"


def main() -> int:
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    report = build_report()
    OUTPUT_JSON.write_text(
        json.dumps(report, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )
    OUTPUT_MD.write_text(write_markdown(report), encoding="utf-8")
    print(f"Wrote: {OUTPUT_JSON}")
    print(f"Wrote: {OUTPUT_MD}")
    failing = [c for c in report["cases"] if c["status"] != "ok"]
    if failing:
        print("Cases needing fixes:")
        for case in failing:
            print(f"- {case['id']}: candidates={case['candidate_count']} expected_min={case['expected_min']}")
    else:
        print("All diagnostic cases met expected_min.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
