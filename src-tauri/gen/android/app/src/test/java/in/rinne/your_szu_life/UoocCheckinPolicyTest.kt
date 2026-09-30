package `in`.rinne.your_szu_life

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class UoocCheckinPolicyTest {
  @Test fun acceptsOnlyOfficialCourseDocuments() {
    assertTrue(isUoocCheckinCourseUrl("https://www.uooc.net.cn/home/course/1588976494"))
    assertTrue(isUoocCheckinCourseUrl("https://www.uooc.net.cn:443/home/course/42#/result"))
  }
  @Test fun rejectsForeignOriginsAndAuthenticationDocuments() {
    for (url in listOf(
      "http://www.uooc.net.cn/home/course/42",
      "https://www.uooc.net.cn.evil.test/home/course/42",
      "https://www.uooc.net.cn:444/home/course/42",
      "https://user:password@www.uooc.net.cn/home/course/42",
      "https://www.uooc.net.cn/user/login",
      "https://www.uooc.net.cn/home/course/0",
      "file:///home/course/42",
    )) assertFalse(url, isUoocCheckinCourseUrl(url))
  }
}
