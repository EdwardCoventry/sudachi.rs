#   Copyright (c) 2023-2024 Works Applications Co., Ltd.
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

class SudachiError(Exception):
    """Base class for all Sudachipy exceptions.
    """
    pass


class ProfileConstraintConflict(SudachiError):
    """A required native identity is unavailable under the selected source."""

    def __init__(self, main_lex: str, native_word_id: int):
        self.main_lex = main_lex
        self.native_word_id = native_word_id
        super().__init__(f"Required native token {native_word_id} is excluded by tokenisation source {main_lex}")
