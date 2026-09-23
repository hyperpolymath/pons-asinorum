function buildAugmented(items: string[]): string {
  let s = "";
  for (const x of items) {
    s += x;
  }
  return s;
}

function buildSelfReferential(items: string[]): string {
  let s = "";
  let i = 0;
  while (i < items.length) {
    s = s + items[i];
    i += 1;
  }
  return s;
}
