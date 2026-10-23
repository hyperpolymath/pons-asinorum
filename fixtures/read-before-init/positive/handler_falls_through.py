# Python deletes the except alias on handler exit: this is a true positive.
def f():
    e = None
    try:
        g()
    except E as e:
        pass
    return e
