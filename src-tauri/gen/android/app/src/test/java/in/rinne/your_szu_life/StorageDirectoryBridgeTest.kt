package `in`.rinne.your_szu_life

import org.junit.Assert.assertEquals
import org.junit.Test

class StorageDirectoryBridgeTest {
  @Test
  fun primaryVolumeMapsToSharedStorageRoot() {
    assertEquals(
        "/storage/emulated/0/Download/泥深",
        resolveTreeDisplayPath("primary:Download/泥深", "/storage/emulated/0", emptyMap(), "fallback"),
    )
  }

  @Test
  fun emptyRelativePathReturnsTheVolumeRoot() {
    assertEquals(
        "/storage/emulated/0",
        resolveTreeDisplayPath("primary:", "/storage/emulated/0", emptyMap(), "fallback"),
    )
  }

  @Test
  fun secondaryVolumeUsesItsRegisteredRoot() {
    assertEquals(
        "/storage/ABCD-1234/记录/泥深",
        resolveTreeDisplayPath(
            "ABCD-1234:记录/泥深",
            "/storage/emulated/0",
            mapOf("ABCD-1234" to "/storage/ABCD-1234"),
            "fallback",
        ),
    )
  }

  @Test
  fun unknownVolumeFallsBackToTheRelativePath() {
    assertEquals(
        "记录/泥深",
        resolveTreeDisplayPath("MISSING:记录/泥深", "/storage/emulated/0", emptyMap(), "fallback"),
    )
  }

  @Test
  fun missingDocumentIdUsesTheTreeUri() {
    assertEquals(
        "content://tree/primary%3ADownload",
        resolveTreeDisplayPath(null, "/storage/emulated/0", emptyMap(), "content://tree/primary%3ADownload"),
    )
  }

  @Test
  fun serializeEscapesQuotesAndNewlines() {
    assertEquals(
        "{\"displayPath\":\"a\\\"b\\nc\",\"treeUri\":\"content://x\",\"hasEntries\":true}",
        serializeDirectoryJson("a\"b\nc", "content://x", true),
    )
  }

  @Test
  fun serializeWritesChineseCharactersVerbatim() {
    assertEquals(
        "{\"displayPath\":\"/sdcard/Download/泥深\",\"treeUri\":\"content://x\",\"hasEntries\":false}",
        serializeDirectoryJson("/sdcard/Download/泥深", "content://x", false),
    )
  }
}
