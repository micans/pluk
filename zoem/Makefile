# The pluk documentation in zoem/aephea format (plan/docstep1.txt).
#
#   make         all outputs in out/: man page, HTML, Markdown
#   make check   lint the sources for Markdown (tools/azmlint.py), then run the console
#                examples of out/*.md (tools/doccheck.py)
#   make update  write the actual output of failing examples back into azm/*.azm
#                (tools/azmupdate.py), then remake the outputs
#   make smoke   test/*.azm as man pages through groff (no warnings allowed) and as
#                HTML through tidy (no errors or warnings allowed)
#   make clean   remove out/

ZOEM     = zoem
PYTHON   = python3
PLUK     = ../pluk/target/release/pluk
CARGO    = ../pluk/Cargo.toml
DATA     = ../doc
OUT      = out

# mac/ first: our copy of the aephea macros shadows the installed aephea (decision 1);
# azm/ for the shared pluk macros (pluk.zmm); out/ for the version stamp.
export ZOEMSEARCHPATH := $(CURDIR)/mac:$(CURDIR)/azm:$(CURDIR)/$(OUT)
export ZOEM

# The documents, by base name of azm/<name>.azm.
DOCS     = SAM-NA

MAN      = $(DOCS:%=$(OUT)/%.7)
HTML     = $(DOCS:%=$(OUT)/%.html)
MD       = $(DOCS:%=$(OUT)/%.md)
MACROS   = $(wildcard mac/*/*.zmm) azm/pluk.zmm
STAMP    = $(OUT)/stamp.version
CSS      = $(OUT)/pluk.css

.PHONY: all check smoke update clean data FORCE

# A failed zoem run (e.g. a reference to an unknown id, item 9) leaves its output
# file; make must not take that for up to date.
.DELETE_ON_ERROR:

all: data $(STAMP) $(CSS) $(MAN) $(HTML) $(MD)

# The version, from pluk's Cargo.toml (decision 9). Rewritten only when it changes, so
# that the outputs are not rebuilt for nothing.
$(STAMP): FORCE | $(OUT)
	@v=$$(sed -n 's/^version *= *"\(.*\)"/\1/p' $(CARGO) | head -n 1); \
	if [ -z "$$v" ]; then echo "no version in $(CARGO)" >&2; exit 1; fi; \
	if [ "$$(cat $@ 2>/dev/null)" != "$$v" ]; then echo "$$v" > $@; echo "version $$v"; fi

# The data files the examples use, beside the documents (decisions 3 and 8).
data: | $(OUT)
	@find $(DATA) -maxdepth 1 -type f ! -name '*.md' -exec cp -p {} $(OUT)/ \;

$(OUT):
	mkdir -p $@

# The stylesheet the HTML pages link to (item 4), shipped beside them.
$(CSS): azm/pluk.css | $(OUT)
	cp $< $@

# zoem runs in out/, where it writes its auxiliary files (<name>.<device>.zmt, the table
# of contents, and .zmr, the references) for the next run to read; tools/runzoem.sh
# reruns it until they are stable.
RUNZOEM  = sh $(CURDIR)/tools/runzoem.sh

$(OUT)/%.7: azm/%.azm $(STAMP) $(MACROS)
	cd $(OUT) && $(RUNZOEM) roff $(CURDIR)/$< $(notdir $@)

$(OUT)/%.html: azm/%.azm $(STAMP) $(MACROS)
	cd $(OUT) && $(RUNZOEM) html $(CURDIR)/$< $(notdir $@)

$(OUT)/%.md: azm/%.azm $(STAMP) $(MACROS)
	cd $(OUT) && $(RUNZOEM) md $(CURDIR)/$< $(notdir $@)

# Smoke test of the macros: test/*.azm as man pages, through groff with all warnings
# and the terminal device (-Tutf8; the default device ps has a font C, so that
# \fC passes there but not in man).
SMOKE    = $(patsubst test/%.azm,$(OUT)/test/%.7,$(wildcard test/*.azm))
SMOKEHTML = $(SMOKE:.7=.html)

# The HTML checks take every page in out/test/, including the further pages of a
# split document (item 9); the data files are copied there for the links to them.
smoke: $(SMOKE) $(SMOKEHTML) $(OUT)/test/pluk.css
	@for f in $(SMOKE); do \
	  w=$$(groff -t -ww -z -Tutf8 -man $$f 2>&1); \
	  if [ -n "$$w" ]; then echo "$$w"; exit 1; fi; \
	done; echo "smoke: groff clean ($(words $(SMOKE)) page(s))"
	@n=0; for f in $(OUT)/test/*.html; do \
	  tidy -q -e $$f || exit 1; n=$$((n + 1)); \
	done; echo "smoke: tidy clean ($$n page(s))"
	@find $(DATA) -maxdepth 1 -type f ! -name '*.md' -exec cp -p {} $(OUT)/test/ \;
	@$(PYTHON) tools/htmllinks.py $(OUT)/test/*.html && echo "smoke: links resolve"

$(OUT)/test/%.7: test/%.azm $(STAMP) $(MACROS) | $(OUT)
	@mkdir -p $(OUT)/test
	cd $(OUT)/test && $(RUNZOEM) roff $(CURDIR)/$< $(notdir $@)

$(OUT)/test/%.html: test/%.azm $(STAMP) $(MACROS) | $(OUT)
	@mkdir -p $(OUT)/test
	cd $(OUT)/test && $(RUNZOEM) html $(CURDIR)/$< $(notdir $@)

$(OUT)/test/pluk.css: azm/pluk.css | $(OUT)
	@mkdir -p $(OUT)/test
	cp $< $@

# azmlint.py: prose lines of the sources that would start Markdown syntax (item 6).
# htmllinks.py: the relative links of all pages, those of split documents included,
# resolve to files and ids (items 8 and 9, decision 8).
check: all
	$(PYTHON) tools/azmlint.py $(DOCS:%=azm/%.azm)
	$(PYTHON) tools/htmllinks.py $(OUT)/*.html
	$(PYTHON) tools/doccheck.py $(PLUK) $(MD)

# The update step (item 7): the expected output of failing examples rewritten in the
# .azm sources from pluk's actual output, then the outputs remade.
update: all
	@st=0; for d in $(DOCS); do \
	  $(PYTHON) tools/azmupdate.py $(PLUK) azm/$$d.azm $(OUT)/$$d.md || st=1; \
	done; $(MAKE) --no-print-directory all; exit $$st

clean:
	rm -rf $(OUT)
