# The with body performs the binding; modelling suppression must not create
# a normal zero-body path for may-unbound.
def f(p):
    with open(p) as fh:
        data = fh.read()
    return data
