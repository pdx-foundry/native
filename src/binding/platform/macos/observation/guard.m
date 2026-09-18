#import <AppKit/AppKit.h>
#import <objc/runtime.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

// Process-local presentation guard. No on-disk game patch and no user input.
static void suppress(id self, SEL selector, id argument) {}
static void suppressActivation(id self, SEL selector, BOOL active) {}
static void suppressNoArg(id self, SEL selector) {}
static void suppressOrder(id self, SEL selector, NSWindowOrderingMode order, NSInteger relative) {}
static void replace(Class cls, SEL selector, IMP imp) {
    Method method = class_getInstanceMethod(cls, selector);
    if (method) method_setImplementation(method, imp);
}
__attribute__((constructor)) static void install(void) {
    @autoreleasepool {
        replace([NSApplication class], @selector(activateIgnoringOtherApps:), (IMP)suppressActivation);
        replace([NSWindow class], @selector(makeKeyAndOrderFront:), (IMP)suppress);
        replace([NSWindow class], @selector(orderFront:), (IMP)suppress);
        replace([NSWindow class], @selector(orderFrontRegardless), (IMP)suppressNoArg);
        replace([NSWindow class], @selector(orderWindow:relativeTo:), (IMP)suppressOrder);
        const char *path = getenv("SPIKE_GUARD_LOG");
        if (path) { FILE *f = fopen(path, "a"); if (f) { fprintf(f, "installed pid=%d arch=arm64\n", getpid()); fclose(f); } }
    }
}
