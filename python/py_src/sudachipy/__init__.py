from .sudachipy import (
    Dictionary,
    Tokenizer,
    SplitMode,
    MorphemeList,
    Morpheme,
    WordInfo,
    PosMatcher,
)
from .config import Config
from . import errors

from importlib import import_module as _import_module
from importlib import metadata as _metadata
from importlib.util import find_spec as _find_spec
import os as _os
from pathlib import Path as _Path

__version__ = "0.6.11-a1"

_REQUIRED_SUDACHIDICT_VERSION_ENV = "SUDACHIPY_REQUIRED_SUDACHIDICT_VERSION"
_SUDACHIDICT_DISTRIBUTIONS = (
    "sudachidict-core",
    "sudachidict-full",
    "sudachidict-small",
)

_DEFAULT_RESOURCEDIR = _Path(__file__).resolve().parent / 'resources'
_DEFAULT_SETTINGFILE = _DEFAULT_RESOURCEDIR / 'sudachi.json'
_DEFAULT_RESOURCEDIR = str(_DEFAULT_RESOURCEDIR.resolve())
_DEFAULT_SETTINGFILE = str(_DEFAULT_SETTINGFILE.resolve())


def _get_absolute_dict_path(dict_type: str) -> str:
    pkg_path = _Path(_import_module(
        f'sudachidict_{dict_type}').__file__).parent
    dic_path = pkg_path / 'resources' / 'system.dic'
    return str(dic_path.resolve())


def _normalise_sudachidict_version(version: str) -> str:
    version = version.strip()
    if version.startswith("v"):
        version = version[1:]
    if ".post" in version:
        version = version.split(".post", 1)[0]
    return version[:8]


def _sudachidict_install_command(expected_version: str) -> str:
    packages = " ".join(
        f"{distribution}=={expected_version}"
        for distribution in _SUDACHIDICT_DISTRIBUTIONS
    )
    return f"python -m pip install --upgrade --force-reinstall --no-deps {packages}"


def _sudachidict_repair_hint(expected_version: str) -> str:
    return (
        f"Run: {_sudachidict_install_command(expected_version)}. "
        "Use --no-deps so the editable SudachiPy fork is not replaced. "
        "Then restart the Python process. "
        "Do not edit hard-coded dictionary IDs until the Sudachi dictionary package versions match."
    )


def _validate_required_dict_version(dict_type: str) -> None:
    required_version = _os.environ.get(_REQUIRED_SUDACHIDICT_VERSION_ENV)
    if not required_version:
        return

    distribution = f"sudachidict-{dict_type}"
    installed_version = _normalise_sudachidict_version(
        _metadata.version(distribution)
    )
    expected_version = _normalise_sudachidict_version(required_version)
    if installed_version != expected_version:
        raise RuntimeError(
            "Sudachi dictionary package version does not match "
            f"{_REQUIRED_SUDACHIDICT_VERSION_ENV}: "
            f"{distribution}={installed_version}, expected={expected_version}. "
            f"{_sudachidict_repair_hint(expected_version)}"
        )


def _find_dict_path(dict_type='core'):
    if dict_type not in ['small', 'core', 'full']:
        raise ValueError('"dict_type" must be "small", "core", or "full".')

    is_installed = _find_spec(f'sudachidict_{dict_type}')
    if is_installed:
        _validate_required_dict_version(dict_type)
        return _get_absolute_dict_path(dict_type)
    else:
        required_version = _os.environ.get(_REQUIRED_SUDACHIDICT_VERSION_ENV)
        if required_version:
            expected_version = _normalise_sudachidict_version(required_version)
            raise ModuleNotFoundError(
                f'Package `sudachidict_{dict_type}` does not exist, but '
                f"{_REQUIRED_SUDACHIDICT_VERSION_ENV}={expected_version} is set. "
                f"{_sudachidict_repair_hint(expected_version)}"
            )
        raise ModuleNotFoundError(
            f'Package `sudachidict_{dict_type}` does not exist. '
            f'You may install it with a command `$ pip install sudachidict_{dict_type}`'
        )
