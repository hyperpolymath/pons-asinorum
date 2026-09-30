# Known-answer twin of negative/opaque_exec.py: identical but for the
# call, so the negative is silent because of `exec`, not by accident.
def f():
    x = 1
    run("print(x)")
    x = 2
    return x
