// Simple example to look at DWARF info

#include <stdio.h>

void use_vars() {
  long a = 3;
  long b = 2;
  long c = a + b;
  a = 4;
}

void greeting() {
  printf("hello, world\n");
}

int main() {
  use_vars();
  greeting();
}
