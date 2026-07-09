#
# Slate javascript source file inclusion
#
# Included by javascript/Makefile
#

content/handlers/javascript/quickjs/bindings.c: \
	$(OBJROOT)/quickjs/binding.h \
	$(OBJROOT)/quickjs/generics.js.inc \
	$(OBJROOT)/quickjs/polyfill.js.inc

BINDINGS := $(wildcard content/handlers/javascript/quickjs/*.bnd)

# Generator for the C include representing the generics.js
$(OBJROOT)/quickjs/generics.js.inc: content/handlers/javascript/quickjs/generics.js $(TOOLROOT)/xxd
	$(Q)$(MKDIR) -p $(OBJROOT)/quickjs
	$(VQ)echo "     XXD: $<"
	$(Q)$(TOOLROOT)/xxd -i $< $@.tmp
	$(Q)sed -e 's/content_handlers_javascript_quickjs_generics_js/generics_js/' $@.tmp > $@

# Generator for the C include representing the polyfill.js
$(OBJROOT)/quickjs/polyfill.js.inc: content/handlers/javascript/quickjs/polyfill.js $(TOOLROOT)/xxd
	$(Q)$(MKDIR) -p $(OBJROOT)/quickjs
	$(VQ)echo "     XXD: $<"
	$(Q)$(TOOLROOT)/xxd -i $< $@.tmp
	$(Q)sed -e 's/content_handlers_javascript_quickjs_polyfill_js/polyfill_js/' $@.tmp > $@

# ensure genbind generates debugging files
GBFLAGS+=-D

$(OBJROOT)/quickjs/binding.h $(OBJROOT)/quickjs/Makefile: content/handlers/javascript/quickjs/slate.bnd $(BINDINGS)
	$(Q)$(MKDIR) -p $(OBJROOT)/quickjs
	$(VQ)echo " GENBIND: $<"
	$(Q)slategenbind $(GBFLAGS) -I content/handlers/javascript/WebIDL $< $(OBJROOT)/quickjs

# create unimplemented report for doxygen
docs/UnimplementedJavascript.md: content/handlers/javascript/quickjs/slate.bnd $(BINDINGS)
	$(Q)$(MKDIR) -p $(OBJROOT)/quickjs
	$(VQ)echo "Unimplemented javascript bindings {#unimplemented}" > $@
	$(VQ)echo "=================================" >> $@
	$(VQ)echo "" >> $@
	$(VQ)echo "This is a list of all the binding methods, getters and setters without an implementation in a binding." >> $@
	$(VQ)echo "" >> $@
	$(VQ)echo "Type | Unimplemented binding" >> $@
	$(VQ)echo "---- | ---------------------" >> $@
	$(VQ)echo " GENBIND: $<"
	$(Q)slategenbind $(GBFLAGS) -Wunimplemented -I content/handlers/javascript/WebIDL $< $(OBJROOT)/quickjs 2>&1 >/dev/null | awk 'BEGIN{count=0} /.*Unimplemented.*/{count++; out=$$4" |"; for(i=5;i<=NF;i++){out=out" "$$i}; print out} END{print "\n",count,"unimplemented bindings"}' >> $@
	$(VQ)echo "" >> $@

ifeq ($(filter $(MAKECMDGOALS),clean test),)
-include $(OBJROOT)/quickjs/Makefile
endif

S_JAVASCRIPT_BINDING:=$(addprefix $(OBJROOT)/quickjs/,$(SLATEGENBIND_SOURCES))

$(S_JAVASCRIPT_BINDING): $(BINDINGS)

QUICKJS_DIR := projects/quickjs
QUICKJS_SOURCES := \
	$(QUICKJS_DIR)/cutils.c \
	$(QUICKJS_DIR)/dtoa.c \
	$(QUICKJS_DIR)/libregexp.c \
	$(QUICKJS_DIR)/libunicode.c \
	$(QUICKJS_DIR)/quickjs.c

S_JAVASCRIPT_BINDING += $(QUICKJS_SOURCES)

S_JAVASCRIPT += content.c quickjs/bindings.c quickjs/api.c

QUICKJS_CFLAGS := -std=gnu99 -I$(QUICKJS_DIR) -D_GNU_SOURCE \
	-DCONFIG_VERSION=\"$(shell cat $(QUICKJS_DIR)/VERSION)\" \
	-Wno-error -Wno-missing-prototypes -Wno-redundant-decls

ifneq ($(HOST),$(BUILD))
QUICKJS_CFLAGS += \
	-DSLATE_QUICKJS_NO_ATOMICS \
	-DSLATE_QUICKJS_NO_FENV \
	-DSLATE_QUICKJS_MATH_COMPAT \
	-DSLATE_QUICKJS_NO_MALLOC_USABLE_SIZE
endif

ifneq ($(findstring amiga,$(HOST))$(filter amiga amigaos3,$(TARGET)),)
QUICKJS_CFLAGS += -DSLATE_QUICKJS_NO_TM_GMTOFF
endif

CFLAGS_content_handlers_javascript_quickjs_api.c += -I$(QUICKJS_DIR)
CFLAGS_projects_quickjs_cutils.c += $(QUICKJS_CFLAGS)
CFLAGS_projects_quickjs_dtoa.c += $(QUICKJS_CFLAGS)
CFLAGS_projects_quickjs_libregexp.c += $(QUICKJS_CFLAGS)
CFLAGS_projects_quickjs_libunicode.c += $(QUICKJS_CFLAGS)
CFLAGS_projects_quickjs_quickjs.c += $(QUICKJS_CFLAGS)
