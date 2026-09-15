function Ratio({ x, divisor }: { x: number; divisor: number }) {
  const r = x / divisor;
  return <span>{r}</span>;
}
