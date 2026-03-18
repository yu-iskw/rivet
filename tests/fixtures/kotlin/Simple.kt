fun sample(value: Int, fallback: Int): Int {
    if (value > 0) {
        return value
    }
    return fallback
}
