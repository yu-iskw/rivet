int sample(int value, int fallback) {
    if (value > 0) {
        return value;
    }
    return fallback;
}
