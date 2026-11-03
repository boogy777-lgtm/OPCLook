# The SDK embeds Expat in this C++ TU; this probe builds the same files as C.
function(filter_embedded_expat list_name)
  set(filtered "${${list_name}}")
  list(FILTER filtered EXCLUDE REGEX "/SOCmn/SOCmnExpat\\.cpp$")
  set(${list_name} "${filtered}" PARENT_SCOPE)
endfunction()
