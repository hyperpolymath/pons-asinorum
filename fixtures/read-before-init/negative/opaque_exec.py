# `exec` can bind `y`, so its read is not judged.
def f(c):
    exec("y = 1")
    if c:
        y = 2
    return y
