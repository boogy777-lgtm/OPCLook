/* Tests the SDK's narrow Expat ABI, namespace and DTD in isolated linkage. */
#include "expat.h"
#include <stdio.h>
#include <string.h>

static void XMLCALL on_start(void* user, const XML_Char* name, const XML_Char** attributes) {
    int* seen = (int*)user;
    (void)attributes;
    if (strcmp(name, "urn:probe|r") == 0) ++*seen;
}
int main(void) {
    const char document[] = "<r xmlns='urn:probe'/>";
    int seen = 0;
    if (sizeof(XML_Char) != 1) return 10;
    XML_Parser parser = XML_ParserCreateNS(NULL, '|');
    if (!parser) return 11;
    if (XML_UseForeignDTD(parser, XML_TRUE) != XML_ERROR_NONE) {
        XML_ParserFree(parser);
        return 12;
    }
    XML_SetUserData(parser, &seen);
    XML_SetStartElementHandler(parser, on_start);
    enum XML_Status status = XML_Parse(parser, document, (int)strlen(document), 1);
    XML_ParserFree(parser);
    if (status != XML_STATUS_OK || seen != 1) return 13;
    puts("SDK Expat narrow ABI, namespace and DTD smoke passed");
    return 0;
}
