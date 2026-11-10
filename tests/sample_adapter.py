"""An adapter used by the CLI tests through --adapter sample_adapter:Echo (a stand-in for a real one)."""
from sf_smartsavant.adapters import StubAdapter


class Echo(StubAdapter):
    name = 'echo'
    synthetic = True

    def __init__(self, prefix='echo'):
        self.prefix = prefix

    def view(self, **kw):
        v = super().view(**kw)
        v['contribution'] = f"{self.prefix}: {v['contribution']}"
        return v
