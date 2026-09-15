def build_augmented(items):
    s = ""
    for x in items:
        s += x
    return s


def build_self_referential(items):
    s = ""
    while items:
        s = s + items.pop()
    return s
