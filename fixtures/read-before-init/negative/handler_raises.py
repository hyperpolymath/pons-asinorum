# The handler never reaches the join, so the implicit del e is dead code.
def f():
    e = None
    try:
        g()
    except E as e:
        raise
    return e
