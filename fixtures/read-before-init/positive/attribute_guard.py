# The same pure None guard bounds both the definition and the use.
def pack(self):
    if self.name is not None:
        name = 1
    if self.name is not None:
        MonsterAddName(name)
