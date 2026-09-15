function sumNumbers(items: number[]): number {
  let n = 0;
  for (const x of items) {
    n += x;
  }
  return n;
}

function Label({ items }: { items: string[] }) {
  let s = "";
  s += "prefix";
  for (const x of items) {
    doThing(x);
  }
  return <span>{s}</span>;
}
