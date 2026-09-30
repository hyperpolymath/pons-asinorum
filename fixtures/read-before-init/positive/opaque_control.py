# Known-answer twin of negative/opaque_exec.py: identical but for the
# call, so the negative is silent because of `exec`, not by accident.
def f(c):
    run("y = 1")
    if c:
        y = 2
    return y
