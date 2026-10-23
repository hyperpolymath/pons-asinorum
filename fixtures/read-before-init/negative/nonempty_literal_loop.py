# A non-empty literal sequence always binds b1 before the loop exits.
def f():
    for sizePrefix in [True, False]:
        b1 = 1
    return b1
