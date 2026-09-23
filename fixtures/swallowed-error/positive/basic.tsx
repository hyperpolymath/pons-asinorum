function Load({ risky }: { risky: () => void }) {
  try {
    risky();
  } catch (e) {}
  return <div />;
}
