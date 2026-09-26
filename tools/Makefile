CXX ?= g++
CXXFLAGS ?= -O2 -std=c++17 -I../TempestSDR/src/include

all: soapy_plugin_smoke soapy_direct_smoke

soapy_plugin_smoke: soapy_plugin_smoke.cpp
	$(CXX) $(CXXFLAGS) -o $@ $< -ldl -pthread

soapy_direct_smoke: soapy_direct_smoke.cpp
	$(CXX) $(CXXFLAGS) -o $@ $< -lSoapySDR

clean:
	rm -f soapy_plugin_smoke soapy_direct_smoke

.PHONY: all clean
