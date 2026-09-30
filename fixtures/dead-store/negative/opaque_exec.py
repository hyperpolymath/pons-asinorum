# `exec` can read any local by name, so no store here is provably dead.
def f():
    x = 1
    exec("print(x)")
    x = 2
    return x
