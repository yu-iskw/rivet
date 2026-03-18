package sample

func sample(value int, fallback int) int {
	if value > 0 {
		return value
	}
	return fallback
}
