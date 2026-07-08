#
# Makefile for NetSurf GTK target
#
# This file is part of NetSurf 
#
# ----------------------------------------------------------------------------
# GTK flag setup (using pkg-config)
# ----------------------------------------------------------------------------

# define additional CFLAGS and LDFLAGS requirements for pkg-configed libs here
SLATE_FEATURE_RSVG_CFLAGS := -DWITH_RSVG
SLATE_FEATURE_VIDEO_CFLAGS := -DWITH_VIDEO

# determine if the rsvg library API version
RSVG_API := $(shell $(PKG_CONFIG) --atleast-version=2.46 librsvg-2.0 && echo 246)
$(eval $(call pkg_config_find_and_add_enabled,RSVG,librsvg-2.0,SVG))
$(eval $(call pkg_config_find_and_add_enabled,VIDEO,gstreamer-0.10,Video))

# GTK and GLIB flags to disable depricated usage
GTKDEPFLAGS := -DG_DISABLE_SINGLE_INCLUDES \
		-DG_DISABLE_DEPRECATED \
	       	-DGTK_DISABLE_SINGLE_INCLUDES \
		-DGTK_MULTIHEAD_SAFE \
		-DPANGO_DISABLE_DEPRECATED

# later editions of gtk 2 deprecate interfaces we rely upon for cursors
# -DGDK_PIXBUF_DISABLE_DEPRECATED

# libsexy currently means we cannot enable this
#		-DGDK_DISABLE_DEPRECATED 

# gtk3 is depricating interfaces we use a lot
ifeq ($(SLATE_GTK_MAJOR),2)
GTKDEPFLAGS += -DGTK_DISABLE_DEPRECATED
endif

# C library API control
ifeq ($(HOST),FreeBSD)
CAPIFLAGS :=
else
CAPIFLAGS := -D_XOPEN_SOURCE=700 \
               -D_POSIX_C_SOURCE=200809L
endif

GTKCFLAGS := -std=c99 -Dgtk -Dslategtk -g \
		$(GTKDEPFLAGS) \
		$(CAPIFLAGS) \
		-D_BSD_SOURCE \
		-D_DEFAULT_SOURCE \
		-D_NETBSD_SOURCE \
		-DGTK_RESPATH=\"$(SLATE_GTK_RES_PATH)\"

# non optional pkg-configed libs
$(eval $(call pkg_config_find_and_add,gtk+-$(SLATE_GTK_MAJOR).0,GTK-$(SLATE_GTK_MAJOR)))
$(eval $(call pkg_config_find_and_add,gthread-2.0,GThread2))
$(eval $(call pkg_config_find_and_add,gmodule-2.0,GModule2))


CFLAGS += $(GTKCFLAGS)
LDFLAGS += -lm

# ---------------------------------------------------------------------------
# Target setup
# ---------------------------------------------------------------------------

# Path to GTK resources
SLATEGTK_RESOURCES_DIR := $(FRONTEND_RESOURCES_DIR)

# The gtk binary target.
EXETARGET := slategtk$(SLATE_GTK_MAJOR)

# The filter and target for split messages
MESSAGES_FILTER=gtk
MESSAGES_TARGET=$(SLATEGTK_RESOURCES_DIR)

# ---------------------------------------------------------------------------
# Windows flag setup
# ---------------------------------------------------------------------------

ifeq ($(HOST),Windows_NT)
    CFLAGS += -U__STRICT_ANSI__
endif

# ----------------------------------------------------------------------------
# Builtin resource handling
# ----------------------------------------------------------------------------

# builtin resource sources
S_RESOURCE :=

# Glib prior to 2.32 does not have GResource handling.
#
# This uses pkg-config to check for the minimum required version for
#  this feature in a way similar to the pkg_config_find_and_add_enabled
#  macro. Note we check for gmodule-2.0 which is a specific part of
#  glib we require.
#
# It would be nice if we could check for this functionality rather
#  than "knowing" the version but there does not appear to be a simple
#  way to implement that.
#
SLATE_FEATURE_GRESOURCE_AVAILABLE := $(shell $(PKG_CONFIG) --atleast-version=2.32 gmodule-2.0 && echo yes)
ifneq (,$(filter $(SLATE_USE_GRESOURCE),AUTO YES))
ifeq ($(SLATE_FEATURE_GRESOURCE_AVAILABLE),yes)

# Gresource use has been enabled
SLATE_FEATURE_GRESOURCE_ENABLED := yes

#resource compiler tool
GLIB_COMPILE_RESOURCES := glib-compile-resources
CFLAGS += -DWITH_GRESOURCE

SLATE_GRESOURCE_XML := $(SLATEGTK_RESOURCES_DIR)/slate.gresource.xml
UI_GRESOURCE_XML := $(SLATEGTK_RESOURCES_DIR)/ui.gresource.xml
MESSAGES_GRESOURCE_XML := $(SLATEGTK_RESOURCES_DIR)/messages.gresource.xml

# generate the netsurf gresource source files
$(OBJROOT)/slate_gresource.c: $(SLATE_GRESOURCE_XML) $(shell $(GLIB_COMPILE_RESOURCES) --sourcedir $(SLATEGTK_RESOURCES_DIR) --generate-dependencies $(SLATE_GRESOURCE_XML))
	$(VQ)echo "GRESORCE: $<"
	$(Q)$(GLIB_COMPILE_RESOURCES) --generate-source --sourcedir $(SLATEGTK_RESOURCES_DIR) --target=$@ $<

S_RESOURCE += $(OBJROOT)/slate_gresource.c

# generate the ui gresource source files
$(OBJROOT)/ui_gresource.c: $(UI_GRESOURCE_XML) $(shell $(GLIB_COMPILE_RESOURCES) --sourcedir $(SLATEGTK_RESOURCES_DIR)/gtk$(SLATE_GTK_MAJOR) --generate-dependencies $(UI_GRESOURCE_XML))
	$(VQ)echo "GRESORCE: $<"
	$(Q)$(GLIB_COMPILE_RESOURCES) --generate-source --sourcedir $(SLATEGTK_RESOURCES_DIR)/gtk$(SLATE_GTK_MAJOR) --target=$@ $<

S_RESOURCE += $(OBJROOT)/ui_gresource.c

# generate the messages gresource source file
$(OBJROOT)/messages_gresource.c: $(MESSAGES_GRESOURCE_XML) $(addsuffix /Messages,$(addprefix $(MESSAGES_TARGET)/,$(MESSAGES_LANGUAGES)))
	$(VQ)echo "GRESORCE: $<"
	$(Q)$(GLIB_COMPILE_RESOURCES) --generate-source --sourcedir $(SLATEGTK_RESOURCES_DIR) --target=$@ $<

S_RESOURCE += $(OBJROOT)/messages_gresource.c


endif
endif

# Build pixbufs as inlines if enabled
ifneq (,$(filter $(SLATE_USE_INLINE_PIXBUF),AUTO YES))
ifneq ($(SLATE_FEATURE_GRESOURCE_ENABLED),yes)

CFLAGS += -DWITH_BUILTIN_PIXBUF

GTK_IMAGE_favicon := favicon.png
GTK_IMAGE_netsurf := slate.xpm
GTK_IMAGE_menu_cursor := menu_cursor.png

# 1: input file
# 2: output file
# 3: bitmap name
define convert_image

# add converted pixbuf to builtin resource sources
S_RESOURCE += $(2)

$(2): $(1)
	$(VQ)echo "  INLINE: ${3}"
	$(Q)echo "#include <gdk-pixbuf/gdk-pixdata.h>" > $(2)
	$(Q)gdk-pixbuf-csource --extern --raw --name=$(3) $(1) >> $(2) || \
	( rm -f $(2) && false )

endef

$(eval $(foreach V,$(filter GTK_IMAGE_%,$(.VARIABLES)),$(call convert_image,$(addprefix $(SLATEGTK_RESOURCES_DIR)/,$($(V))),$(OBJROOT)/$(patsubst GTK_IMAGE_%,%,$(V)).c,$(patsubst GTK_IMAGE_%,%,$(V))_pixdata)))
endif
endif

# ----------------------------------------------------------------------------
# Source file setup
# ----------------------------------------------------------------------------

# S_FRONTEND are sources purely for the GTK frontend
S_FRONTEND := gui.c misc.c schedule.c layout_pango.c bitmap.c plotters.c \
	scaffolding.c gdk.c completion.c throbber.c accelerator.c        \
	selection.c window.c fetch.c download.c menu.c print.c		 \
	search.c tabs.c toolbar.c gettext.c compat.c viewdata.c		 \
	viewsource.c preferences.c about.c resources.c corewindow.c	 \
	local_history.c global_history.c cookies.c hotlist.c page_info.c

# This is the final source build list
# Note this is deliberately *not* expanded here as common and image
#   are not yet available
SOURCES = $(S_COMMON) $(S_IMAGE) $(S_BROWSER) $(S_RESOURCE) $(S_FRONTEND)

# ----------------------------------------------------------------------------
# Install target
# ----------------------------------------------------------------------------

GTK_RESOURCES_LIST := \
	languages SearchEngines ca-bundle.txt \
	default.css adblock.css quirks.css internal.css \
	credits.html licence.html welcome.html Messages \
	default.ico favicon.png slate.png slate.xpm slate-16x16.xpm

GTK_RESOURCES_LIST := \
	$(addprefix $(SLATEGTK_RESOURCES_DIR)/, $(GTK_RESOURCES_LIST)) \
	$(wildcard $(SLATEGTK_RESOURCES_DIR)/gtk$(SLATE_GTK_MAJOR)/*.ui)

# translations with more than just Messages files
GTK_TRANSLATIONS_HTML := de en fr it ja nl zh_CN

# destination for installed resources is the first entry in the gtk resource path
SLATEGTK_RESOURCES_DESTDIR := $(DESTDIR)$(word 1,$(subst :, ,$(SLATE_GTK_RES_PATH)))

install-gtk:
	$(Q)$(MKDIR) -p $(DESTDIR)$(SLATE_GTK_BIN)
	$(Q)install $(EXETARGET) $(DESTDIR)$(SLATE_GTK_BIN)slate-gtk$(SUBTARGET)
	$(Q)$(MKDIR) -p $(SLATEGTK_RESOURCES_DESTDIR)/icons
	$(Q)install -m 0644 $(SLATEGTK_RESOURCES_DIR)/icons/*.png $(SLATEGTK_RESOURCES_DESTDIR)/icons
	$(Q)$(MKDIR) -p $(SLATEGTK_RESOURCES_DESTDIR)/throbber
	$(Q)install -m 0644 $(SLATEGTK_RESOURCES_DIR)/throbber/*.png $(SLATEGTK_RESOURCES_DESTDIR)/throbber
	$(Q)tar -c -h -C $(SLATEGTK_RESOURCES_DIR) -f - $(GTK_TRANSLATIONS_HTML) | tar -xv -C $(SLATEGTK_RESOURCES_DESTDIR) -f -
	$(Q)install -m 0644 $(GTK_RESOURCES_LIST) $(SLATEGTK_RESOURCES_DESTDIR)

# ----------------------------------------------------------------------------
# Package target
# ----------------------------------------------------------------------------

package-gtk:
