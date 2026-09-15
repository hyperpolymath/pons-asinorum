function Load({ risky }: { risky: () => void }) {
  try {
    risky();
  } catch (e) {
    console.error(e);
  }
  return <div />;
}
