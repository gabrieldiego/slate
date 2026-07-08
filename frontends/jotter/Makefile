#
# Makefile for the Slate Jotter target
#
# This file is part of NetSurf 

# ----------------------------------------------------------------------------
# Jotter flag setup (using pkg-config)
# ----------------------------------------------------------------------------

CWARNFLAGS += -Werror

CFLAGS += -std=c99 \
	  -Djotter -g \
	  -DJOTTER_RESPATH=\"$(SLATE_JOTTER_RESOURCES)\"

LDFLAGS += -lm

# ---------------------------------------------------------------------------
# Target setup
# ---------------------------------------------------------------------------

# Path to jotter resources
JOTTER_RESOURCES_DIR := $(FRONTEND_RESOURCES_DIR)

# The filter and target for split messages
MESSAGES_FILTER=jotter
MESSAGES_TARGET=$(JOTTER_RESOURCES_DIR)

# ---------------------------------------------------------------------------
# HOST specific feature flags
# ---------------------------------------------------------------------------

# enable POSIX and XSI features.
# everywhere but freebsd where the default set already has them enabled
ifneq ($(HOST),FreeBSD)
  CFLAGS += -D_POSIX_C_SOURCE=200809L \
	    -D_XOPEN_SOURCE=700 \
	    -D_BSD_SOURCE \
	    -D_DEFAULT_SOURCE \
	    -D_NETBSD_SOURCE
endif

# Windows flag setup
ifeq ($(HOST),Windows_NT)
  CFLAGS += -U__STRICT_ANSI__
endif

# ----------------------------------------------------------------------------
# Source file setup
# ----------------------------------------------------------------------------

# S_FRONTEND are sources purely for the Jotter build
S_FRONTEND := main.c output.c filetype.c schedule.c bitmap.c plot.c browser.c \
	download.c 401login.c layout.c dispatch.c fetch.c


# This is the final source build list
# Note this is deliberately *not* expanded here as common and image
#   are not yet available
SOURCES = $(S_COMMON) $(S_IMAGE) $(S_BROWSER) $(S_PDF) $(S_FRONTEND)
EXETARGET := jotter

# ----------------------------------------------------------------------------
# Install target
# ----------------------------------------------------------------------------

install-jotter:

# ----------------------------------------------------------------------------
# Package target
# ----------------------------------------------------------------------------

package-jotter:
