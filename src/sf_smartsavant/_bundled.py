"""Files shipped inside the wheel (copies of spec/; tests/test_vectors.py fails if a copy differs)."""
from __future__ import annotations

from importlib import resources


def path(*parts: str):
    return resources.files('sf_smartsavant').joinpath('data', *parts)


def read_text(*parts: str) -> str:
    return path(*parts).read_text(encoding='utf-8')


def schema_path(name: str) -> str:
    """Path of a JSON schema shipped in the package: 'view' or 'merge' (for adapters that constrain
    decoding with it)."""
    return str(path('schema', f'{name}.schema.json'))
