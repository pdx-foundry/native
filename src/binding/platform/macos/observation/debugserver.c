// LLDB requests a separate group and passes --setsid to its stub. Native already isolates
// the worker from terminal signals, and must retain the stub in that group for cleanup.
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int main(int argc, char **argv) {
    const char *debugserver = getenv("PDX_NATIVE_DEBUGSERVER");
    pid_t worker = getppid();
    if (!debugserver || worker <= 1 || getpgid(worker) != worker) {
        fputs("debugserver launcher has no owned worker group\n", stderr);
        return 1;
    }
    if (setpgid(0, worker) < 0) {
        perror("joining worker group");
        return 1;
    }
    int kept = 1;
    for (int i = 1; i < argc; ++i) {
        if (strcmp(argv[i], "--setsid") != 0) {
            argv[kept++] = argv[i];
        }
    }
    argv[kept] = NULL;
    argv[0] = (char *)debugserver;
    execv(debugserver, argv);
    perror("starting debugserver");
    return 1;
}
