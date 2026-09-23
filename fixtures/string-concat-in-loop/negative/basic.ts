function sumNumbers(items: number[]): number {
  let n = 0;
  for (const x of items) {
    n += x;
  }
  return n;
}

function buildOnce(items: string[]): string {
  let s = "";
  s += "prefix";
  for (const x of items) {
    doThing(x);
  }
  return s;
}

function buildInNestedFunction(items: string[]): void {
  for (const x of items) {
    const render = (): string => {
      let t = "";
      t += "z";
      return t;
    };
    render();
  }
}
