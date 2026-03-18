# LD Game engine
APPNAME := "game"

#Default to linux but can be overriden by the commandline
BUILDFORPLATFORM := linux

ALLDIRS := ${shell find src -type d -print}

SRCDIR := src
BINDIR := bin

SRCFILES = $(wildcard src/ecs/*.cpp)

#Linux
ifeq (linux, $(BUILDFORPLATFORM))
	LINKER := clang++
	LFLAGS := -lSDL2 -lSDL2_mixer -lSDL2_ttf -lSDL2_image
	CXX := clang++
	CXXFLAGS := -std=c++17
	LDLIBS :=-lglfw -lGL

	SRCFILES += $(wildcard src/platform/desktop/*.cpp)

	BINDIR := bin/linux
	EXT :=

endif

ifeq (windows, $(BUILDFORPLATFORM))
	LINKER := /usr/bin/x86_64-w64-mingw32-c++
	LFLAGS := -static-libstdc++ -static-libgcc -lSDL2 -lSDL2_image -lSDL2_mixer -lSDL2_ttf -I /winlib/include -L /winlib/lib
	CXX := /usr/bin/x86_64-w64-mingw32-c++
	CXXFLAGS := -std=c++17 -I /winlib/include

	SRCFILES += $(wildcard src/platform/desktop/*.cpp)

	BINDIR := bin/windows
	EXT := .exe
endif

ifeq (web, $(BUILDFORPLATFORM))
	LINKER := emcc
	LFLAGS := --shell-file src/platform/web/index.html -s USE_SDL=2 -s USE_SDL_MIXER=2 -s USE_SDL_TTF=2 -s USE_SDL_IMAGE=2
	CXX := emcc
	CXXFLAGS := -std=c++17

	SRCFILES += $(wildcard src/platform/web/*.cpp)

	BINDIR := bin/web
	EXT := .html
endif

ifeq (macos, $(BUILDFORPLATFORM))
	LINKER := o64-clang++
	LFLAGS := -lSDL2 -lSDL2_mixer -lSDL2_image -lSDL2_ttf -L /osxcross/target/macports/pkgs/opt/local/lib -I /osxcross/target/macports/pkgs/opt/local/include -stdlib=libc++
	CXX := o64-clang++
	CXXFLAGS := -std=c++17 -stdlib=libc++  

	SRCFILES += $(wildcard src/platform/desktop/*.cpp)

	BINDIR := bin/macos
	EXT :=
endif

OBJS = $(SRCFILES:.cpp=.o)

$(BINDIR)/$(APPNAME)$(EXT): $(OBJS)
	mkdir -p $(BINDIR)
	$(CXX) -o $@ $^ $(LFLAGS)
	rm -f $(OBJS)
	#Fix for bug in bldr leaving root as owner of folder
	chown -R 1000:1000 bin

.PHONY: clean
clean:
	rm -f $(OBJS)
	rm -f $(BINDIR)/$(APPNAME)$(EXT)
