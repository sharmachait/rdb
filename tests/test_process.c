#include <unistd.h>
#include <stdio.h>

int main() {
    printf("Starting program, PID: %d\n", getpid());
    int x = 0;
    while(1) {
        printf("Looping... x = %d\n", x);
        x++;
        sleep(2);
    }
    return 0;
}