# A suppressible exception can resume after the with body from any point.
import contextlib

def f(d, k):
    x = None
    with contextlib.suppress(KeyError):
        x = d[k]
    return x
