package `in`.rinne.your_szu_life

internal fun browserTitle(title: String?): String = title?.trim()?.takeIf { it.isNotEmpty() } ?: "泥深"

internal fun browserBottomInset(systemBar: Int, cutout: Int, keyboard: Int): Int =
  maxOf(systemBar, cutout, keyboard)
