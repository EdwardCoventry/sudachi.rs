#   Copyright (c) 2021 Works Applications Co., Ltd.
#
#   Licensed under the Apache License, Version 2.0 (the "License");
#   you may not use this file except in compliance with the License.
#   You may obtain a copy of the License at
#
#       http://www.apache.org/licenses/LICENSE-2.0
#
#    Unless required by applicable law or agreed to in writing, software
#   distributed under the License is distributed on an "AS IS" BASIS,
#   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
#   See the License for the specific language governing permissions and
#   limitations under the License.

import os
import unittest
from unittest import mock


class TestImport(unittest.TestCase):
    def test_import_dictionary(self):
        from sudachipy.dictionary import Dictionary
        self.assertIsNotNone(Dictionary)

    def test_import_morpheme(self):
        from sudachipy.morpheme import Morpheme
        self.assertIsNotNone(Morpheme)

    def test_import_morphemelist(self):
        from sudachipy.morphemelist import MorphemeList
        self.assertIsNotNone(MorphemeList)


class TestRequiredSudachiDictVersion(unittest.TestCase):
    def test_validate_required_dict_version_is_noop_without_env(self):
        import sudachipy

        with mock.patch.object(
            sudachipy._metadata,
            "version",
            side_effect=AssertionError("metadata should not be read"),
        ):
            with mock.patch.dict(os.environ, {}, clear=True):
                sudachipy._validate_required_dict_version("full")

    def test_validate_required_dict_version_accepts_matching_version(self):
        import sudachipy

        with mock.patch.object(
            sudachipy._metadata,
            "version",
            return_value="20260116.post1",
        ):
            with mock.patch.dict(
                os.environ,
                {sudachipy._REQUIRED_SUDACHIDICT_VERSION_ENV: "20260116"},
            ):
                sudachipy._validate_required_dict_version("full")

    def test_validate_required_dict_version_rejects_mismatch(self):
        import sudachipy

        with mock.patch.object(
            sudachipy._metadata,
            "version",
            return_value="20240716",
        ):
            with mock.patch.dict(
                os.environ,
                {sudachipy._REQUIRED_SUDACHIDICT_VERSION_ENV: "20260116"},
            ):
                with self.assertRaisesRegex(
                    RuntimeError,
                    (
                        "sudachidict-full=20240716, expected=20260116.*"
                        "python -m pip install --upgrade --force-reinstall --no-deps "
                        "sudachidict-core==20260116 sudachidict-full==20260116 "
                        "sudachidict-small==20260116.*"
                        "Do not edit hard-coded dictionary IDs"
                    ),
                ):
                    sudachipy._validate_required_dict_version("full")

    def test_find_dict_path_missing_package_error_has_repair_command(self):
        import sudachipy

        with mock.patch.object(sudachipy, "_find_spec", return_value=None):
            with mock.patch.dict(
                os.environ,
                {sudachipy._REQUIRED_SUDACHIDICT_VERSION_ENV: "20260116"},
            ):
                with self.assertRaisesRegex(
                    ModuleNotFoundError,
                    (
                        "Package `sudachidict_full` does not exist.*"
                        "SUDACHIPY_REQUIRED_SUDACHIDICT_VERSION=20260116.*"
                        "python -m pip install --upgrade --force-reinstall --no-deps "
                        "sudachidict-core==20260116 sudachidict-full==20260116 "
                        "sudachidict-small==20260116.*"
                        "restart the Python process"
                    ),
                ):
                    sudachipy._find_dict_path("full")
