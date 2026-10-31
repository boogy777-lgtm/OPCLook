// The SDK consumes Expat from C++ while the implementation is compiled as C.
#include "expat.h"
#include <type_traits>
static_assert(std::is_same<XML_Char, char>::value, "SDK expects narrow Expat strings");
using ExpectedStart = void (XMLCALL *)(void*, const char*, const char**);
static_assert(std::is_same<XML_StartElementHandler, ExpectedStart>::value,
              "Expat callback calling convention changed");
int main() {
    XML_Parser parser = XML_ParserCreate(nullptr);
    if (!parser) return 1;
    XML_ParserFree(parser);
    return 0;
}
