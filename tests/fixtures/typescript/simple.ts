function sample(value: number, fallback: number): number {
  if (value > 0) {
    return value;
  }
  return fallback;
}
