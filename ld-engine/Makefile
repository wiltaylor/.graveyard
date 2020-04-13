# LD Game engine
APPNAME := "game"

#Default to linux but can be overriden by the commandline
BUILDFORPLATFORM := linux

#Global source files
SRCFILES :=

ALLDIRS := ${shell find src -type d -print}

SRCDIR := src
OBJDIR := obj
BINDIR := bin

#Linux
ifeq (linux, $(BUILDFORPLATFORM))
	LINKER := clang++
	LFLAGS := -lSDL2
	CXX := clang++
	CXXFLAGS := -std=c++17
	LDLIBS :=-lglfw -lGL

	SRCFILES +=	src/platform/desktop/p_main.cpp
	OBJDIR := obj/linux
	BINDIR := bin/linux
	EXT :=

endif

ifeq (windows, $(BUILDFORPLATFORM))
	LINKER := /usr/bin/x86_64-w64-mingw32-c++
	LFLAGS := -static-libstdc++ -static-libgcc -lSDL2
	CXX := /usr/bin/x86_64-w64-mingw32-c++
	CXXFLAGS := -std=c++17

	SRCFILES += src/platform/desktop/p_main.cpp
	OBJDIR := obj/windows
	BINDIR := bin/windows
	EXT := .exe
endif

ifeq (web, $(BUILDFORPLATFORM))
	LINKER := emcc
	LFLAGS := --shell-file src/platform/web/index.html -s USE_SDL=2
	CXX := emcc
	CXXFLAGS := -std=c++17

	SRCFILES += src/platform/web/p_main.cpp
	OBJDIR := obj/web
	BINDIR := bin/web
	EXT := .html
endif

ifeq (macos, $(BUILDFORPLATFORM))
	LINKER := o64-clang++
	LFLAGS := -lSDL2
	CXX := o64-clang++
	CXXFLAGS := -std=c++17

	SRCFILES += src/platform/desktop/p_main.cpp
	OBJDIR := obj/macos
	BINDIR := bin/macos
	EXT :=
endif

OBJS := $(SRCFILES:$(SRCDIR)/%.cpp=$(OBJDIR)/%.o)
OBJDIRS := $(ALLDIRS:src/%=$(OBJDIR)/%)

$(BINDIR)/$(APPNAME)$(EXT): $(BINDIR) $(OBJDIRS) $(OBJS)
	@$(LINKER) $(OBJS) $(LFLAGS) -o $@

$(OBJS): $(OBJDIR)/%.o : $(SRCFILES)
	@$(CXX) $(CXXFLAGS) -c $< -o $@

$(BINDIR):
	mkdir -p $(BINDIR)

$(OBJDIRS):
	mkdir -p $(OBJDIRS)

.PHONY: clean
clean:
	rm -f $(OBJS)
	rm -f $(BINDIR)/$(APPNAME)$(EXT)
