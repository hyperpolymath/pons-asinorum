def sum_numbers(items):
    n = 0
    for x in items:
        n += x
    return n


def build_once(items):
    s = ""
    s += "prefix"
    for x in items:
        do_thing(x)
    return s


def build_in_nested_function(items):
    for x in items:
        def render():
            t = ""
            t += "z"
            return t

        render()
