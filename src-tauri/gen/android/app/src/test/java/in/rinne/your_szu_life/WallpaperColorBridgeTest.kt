package `in`.rinne.your_szu_life

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Pure-function tests; no emulator or Robolectric needed. */
class WallpaperColorBridgeTest {
  @Test
  fun formatsArgbAsLowercaseHexWithoutAlpha() {
    assertEquals("#5d5fef", formatHex(0xFF5D5FEF.toInt()))
    assertEquals("#000000", formatHex(0xFF000000.toInt()))
    assertEquals("#ffffff", formatHex(0xFFFFFFFF.toInt()))
    assertEquals("#0e6c4a", formatHex(0xFF0E6C4A.toInt()))
  }

  @Test
  fun padsShortChannels() {
    assertEquals("#0000ff", formatHex(0xFF0000FF.toInt()))
    assertEquals("#00ff00", formatHex(0xFF00FF00.toInt()))
  }

  @Test
  fun emitsNullSeedWhenPlatformHasNoColour() {
    assertEquals(
        """{"seed":null,"source":"unavailable","dark":false}""",
        seedJson(null, "unavailable", false),
    )
  }

  @Test
  fun emitsSeedAndSource() {
    val json = seedJson(0xFF5D5FEF.toInt(), "wallpaper", true)
    assertEquals("""{"seed":"#5d5fef","source":"wallpaper","dark":true}""", json)
    assertTrue(json.contains("\"dark\":true"))
  }

  @Test
  fun emitsDarkFalseExplicitly() {
    val json = seedJson(0xFF123456.toInt(), "system-accent", false)
    assertTrue(json.contains("\"dark\":false"))
    assertFalse(json.contains("\"dark\":true"))
  }
}
