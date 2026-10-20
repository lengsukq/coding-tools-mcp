#import <Foundation/Foundation.h>
#import <AppKit/AppKit.h>
#import <ScreenCaptureKit/ScreenCaptureKit.h>
#import <CoreGraphics/CoreGraphics.h>
#import <ApplicationServices/ApplicationServices.h>
#import <Security/Security.h>
#import <dispatch/dispatch.h>

// All public bridge results are malloc-owned UTF-8 JSON; Rust must call
// computer_native_free(). No screenshots are persisted to the file system.
static char *reply(NSDictionary *object) {
    NSData *data = [NSJSONSerialization dataWithJSONObject:object options:0 error:nil];
    if (!data) return strdup("{\"ok\":false,\"error\":\"NATIVE_JSON_ERROR\"}");
    char *result = malloc(data.length + 1);
    if (!result) return NULL;
    memcpy(result, data.bytes, data.length);
    result[data.length] = 0;
    return result;
}

static void findPressTargets(AXUIElementRef element, NSString *wantedLabel, NSString *wantedRole,
                             int depth, NSUInteger *budget, NSMutableArray *results) {
    if (depth > 6 || *budget == 0 || results.count > 1) return;
    --(*budget);
    CFTypeRef role = NULL, title = NULL, label = NULL;
    AXUIElementCopyAttributeValue(element, kAXRoleAttribute, &role);
    AXUIElementCopyAttributeValue(element, kAXTitleAttribute, &title);
    AXUIElementCopyAttributeValue(element, kAXDescriptionAttribute, &label);
    NSString *roleText = role && CFGetTypeID(role) == CFStringGetTypeID() ? (__bridge NSString *)role : @"";
    NSString *titleText = title && CFGetTypeID(title) == CFStringGetTypeID() ? (__bridge NSString *)title : @"";
    NSString *labelText = label && CFGetTypeID(label) == CFStringGetTypeID() ? (__bridge NSString *)label : @"";
    BOOL matches = ([titleText isEqualToString:wantedLabel] || [labelText isEqualToString:wantedLabel]) &&
                   (wantedRole.length == 0 || [roleText isEqualToString:wantedRole]);
    if (matches) [results addObject:(__bridge id)element];
    if (role) CFRelease(role);
    if (title) CFRelease(title);
    if (label) CFRelease(label);
    if (depth >= 6 || results.count > 1) return;
    CFTypeRef children = NULL;
    if (AXUIElementCopyAttributeValue(element, kAXChildrenAttribute, &children) != kAXErrorSuccess || !children) return;
    if (CFGetTypeID(children) == CFArrayGetTypeID()) {
        for (id child in (__bridge NSArray *)children) {
            if (*budget == 0 || results.count > 1) break;
            findPressTargets((__bridge AXUIElementRef)child, wantedLabel, wantedRole, depth + 1, budget, results);
        }
    }
    CFRelease(children);
}

static char *failure(NSString *code, NSString *details);
static BOOL isScreenLocked(void);
static BOOL pointHitsApprovedWindow(CGPoint point, pid_t approvedPID);

// No capture or Accessibility traversal is performed here. This reports the
// CURRENT process identity used by macOS TCC, which can differ between
// "tauri dev", a build-folder .app and the app installed in /Applications.
char *computer_native_screen_diagnostics(void) {
    @autoreleasepool {
        NSBundle *bundle = NSBundle.mainBundle;
        NSString *bundlePath = bundle.bundlePath ?: @"";
        NSString *executable = bundle.executablePath ?: @"";
        NSString *bundleID = bundle.bundleIdentifier ?: @"";
        BOOL isApp = [bundlePath.pathExtension.lowercaseString isEqualToString:@"app"];
        NSMutableDictionary *diagnostics = [@{
            @"ok": @YES,
            @"bundle_id": bundleID,
            @"bundle_path": bundlePath,
            @"executable_path": executable,
            @"process_name": NSProcessInfo.processInfo.processName ?: @"",
            @"process_id": @(NSProcessInfo.processInfo.processIdentifier),
            @"is_app_bundle": @(isApp),
            @"preflight_granted": @(CGPreflightScreenCaptureAccess()),
            @"capture_usage_description_present": @([bundle objectForInfoDictionaryKey:@"NSScreenCaptureUsageDescription"] != nil),
            @"expected_bundle_id": @"com.codingtools.mcp.desktop"
        } mutableCopy];

        // Public Security.framework code-signing APIs inspect only this app's
        // own signature. No shell command, TCC database access or screen read.
        SecStaticCodeRef staticCode = NULL;
        OSStatus status = SecStaticCodeCreateWithPath((__bridge CFURLRef)bundle.bundleURL,
                                                      kSecCSDefaultFlags, &staticCode);
        if (status == errSecSuccess && staticCode) {
            CFDictionaryRef signingInfo = NULL;
            status = SecCodeCopySigningInformation(staticCode, kSecCSSigningInformation,
                                                   &signingInfo);
            if (status == errSecSuccess && signingInfo) {
                NSDictionary *info = (__bridge NSDictionary *)signingInfo;
                NSNumber *flags = info[(__bridge NSString *)kSecCodeInfoFlags];
                NSArray *certificates = info[(__bridge NSString *)kSecCodeInfoCertificates];
                NSString *team = info[(__bridge NSString *)kSecCodeInfoTeamIdentifier];
                NSString *identifier = info[(__bridge NSString *)kSecCodeInfoIdentifier];
                BOOL adHoc = flags && ((flags.unsignedIntValue & 0x00000002u) != 0);
                BOOL certificateBacked = [certificates isKindOfClass:NSArray.class] &&
                                         certificates.count > 0;
                diagnostics[@"signature_type"] = adHoc ? @"ad_hoc" :
                    (certificateBacked ? @"certificate" : @"unsigned_or_unknown");
                diagnostics[@"signing_team_id"] = [team isKindOfClass:NSString.class] ? team : @"";
                diagnostics[@"signing_identifier"] = [identifier isKindOfClass:NSString.class] ? identifier : @"";
                CFRelease(signingInfo);
            }
            CFRelease(staticCode);
        }
        if (!diagnostics[@"signature_type"]) {
            diagnostics[@"signature_type"] = @"unavailable";
            diagnostics[@"signature_diagnostic_error"] = @(status);
        }
        return reply(diagnostics);
    }
}

char *computer_native_available_apps(void) {
    @autoreleasepool {
        NSMutableArray *apps = [NSMutableArray array];
        for (NSRunningApplication *app in NSWorkspace.sharedWorkspace.runningApplications) {
            if (app.activationPolicy != NSApplicationActivationPolicyRegular || !app.bundleIdentifier) continue;
            if (apps.count >= 60) break;
            [apps addObject:@{@"bundle_id": app.bundleIdentifier, @"name": app.localizedName ?: app.bundleIdentifier}];
        }
        [apps sortUsingComparator:^NSComparisonResult(NSDictionary *a, NSDictionary *b) {
            return [a[@"name"] localizedStandardCompare:b[@"name"]];
        }];
        return reply(@{@"ok": @YES, @"applications": apps});
    }
}

char *computer_native_frontmost(void) {
    @autoreleasepool {
        NSRunningApplication *app = NSWorkspace.sharedWorkspace.frontmostApplication;
        if (!app || !app.bundleIdentifier) return failure(@"NO_FRONTMOST_APP", @"No foreground application is available");
        return reply(@{@"ok": @YES, @"bundle_id": app.bundleIdentifier,
                       @"app_name": app.localizedName ?: @"", @"pid": @(app.processIdentifier)});
    }
}

bool computer_native_accessibility_granted(void) {
    return AXIsProcessTrusted();
}

bool computer_native_request_accessibility(void) {
    return AXIsProcessTrustedWithOptions((__bridge CFDictionaryRef)@{(__bridge NSString *)kAXTrustedCheckOptionPrompt: @YES});
}

// Any inaccessible/unknown focused element is treated as protected: never
// turn an Accessibility failure into permission to inject keyboard input.
static BOOL isSecureFocusedField(pid_t pid) {
    AXUIElementRef application = AXUIElementCreateApplication(pid);
    if (!application) return YES;
    AXUIElementSetMessagingTimeout(application, 0.5);
    CFTypeRef focused = NULL;
    BOOL secure = YES;
    if (AXUIElementCopyAttributeValue(application, kAXFocusedUIElementAttribute, &focused) == kAXErrorSuccess
        && focused && CFGetTypeID(focused) == AXUIElementGetTypeID()) {
        CFTypeRef role = NULL, subrole = NULL;
        BOOL gotRole = AXUIElementCopyAttributeValue((AXUIElementRef)focused, kAXRoleAttribute, &role) == kAXErrorSuccess;
        BOOL gotSubrole = AXUIElementCopyAttributeValue((AXUIElementRef)focused, kAXSubroleAttribute, &subrole) == kAXErrorSuccess;
        if (gotRole && role && CFGetTypeID(role) == CFStringGetTypeID()) {
            NSString *roleText = (__bridge NSString *)role;
            NSString *subroleText = gotSubrole && subrole && CFGetTypeID(subrole) == CFStringGetTypeID()
                ? (__bridge NSString *)subrole : @"";
            secure = [roleText isEqualToString:@"AXSecureTextField"] ||
                     [subroleText isEqualToString:@"AXSecureTextField"];
        }
        if (role) CFRelease(role);
        if (subrole) CFRelease(subrole);
    }
    if (focused) CFRelease(focused);
    CFRelease(application);
    return secure;
}

// The UI tree deliberately never reads AXValue, which may be passwords or user
// input. Only labels, titles, roles, bounds and the available action names.
static NSMutableArray *treeForElement(AXUIElementRef element, NSUInteger depth, NSUInteger *budget) {
    NSMutableArray *output = [NSMutableArray array];
    if (depth > 4 || *budget == 0) return output;
    CFTypeRef children = NULL;
    if (AXUIElementCopyAttributeValue(element, kAXChildrenAttribute, &children) != kAXErrorSuccess || !children)
        return output;
    if (CFGetTypeID(children) != CFArrayGetTypeID()) { CFRelease(children); return output; }
    NSArray *childArray = (__bridge NSArray *)children;
    for (id child in childArray) {
        if (*budget == 0) break;
        AXUIElementRef childElement = (__bridge AXUIElementRef)child;
        CFTypeRef role = NULL, title = NULL, label = NULL, framePosition = NULL, frameSize = NULL;
        AXUIElementCopyAttributeValue(childElement, kAXRoleAttribute, &role);
        AXUIElementCopyAttributeValue(childElement, kAXTitleAttribute, &title);
        AXUIElementCopyAttributeValue(childElement, kAXDescriptionAttribute, &label);
        AXUIElementCopyAttributeValue(childElement, kAXPositionAttribute, &framePosition);
        AXUIElementCopyAttributeValue(childElement, kAXSizeAttribute, &frameSize);
        NSString *roleText = role && CFGetTypeID(role) == CFStringGetTypeID() ? (__bridge NSString *)role : @"";
        NSString *titleText = title && CFGetTypeID(title) == CFStringGetTypeID() ? (__bridge NSString *)title : @"";
        NSString *labelText = label && CFGetTypeID(label) == CFStringGetTypeID() ? (__bridge NSString *)label : @"";
        CGPoint origin = CGPointZero;
        CGSize size = CGSizeZero;
        if (framePosition && CFGetTypeID(framePosition) == AXValueGetTypeID()) AXValueGetValue((AXValueRef)framePosition, kAXValueCGPointType, &origin);
        if (frameSize && CFGetTypeID(frameSize) == AXValueGetTypeID()) AXValueGetValue((AXValueRef)frameSize, kAXValueCGSizeType, &size);
        NSMutableDictionary *item = [@{@"role": roleText, @"title": [titleText substringToIndex:MIN(160, titleText.length)],
            @"label": [labelText substringToIndex:MIN(160, labelText.length)],
            @"x": @(origin.x), @"y": @(origin.y), @"width": @(size.width), @"height": @(size.height)} mutableCopy];
        --(*budget);
        if (depth < 4) item[@"children"] = treeForElement(childElement, depth + 1, budget);
        [output addObject:item];
        if (role) CFRelease(role);
        if (title) CFRelease(title);
        if (label) CFRelease(label);
        if (framePosition) CFRelease(framePosition);
        if (frameSize) CFRelease(frameSize);
    }
    CFRelease(children);
    return output;
}

char *computer_native_ui_tree(const char *expectedBundle) {
    @autoreleasepool {
        if (isScreenLocked()) return failure(@"SCREEN_LOCKED", @"Unlock the desktop before reading accessibility UI");
        if (!AXIsProcessTrusted()) return failure(@"ACCESSIBILITY_PERMISSION_REQUIRED", @"Enable Accessibility permission in macOS Settings");
        NSRunningApplication *app = NSWorkspace.sharedWorkspace.frontmostApplication;
        if (!app) return failure(@"NO_FRONTMOST_APP", @"No foreground application");
        if (expectedBundle && ![app.bundleIdentifier isEqualToString:[NSString stringWithUTF8String:expectedBundle]])
            return failure(@"TARGET_APP_MISMATCH", @"Only the locally approved application may expose its UI tree");
        AXUIElementRef root = AXUIElementCreateApplication(app.processIdentifier);
        if (!root) return failure(@"AX_UNAVAILABLE", @"Unable to access application UI");
        NSUInteger budget = 150;
        NSMutableArray *tree = treeForElement(root, 0, &budget);
        CFRelease(root);
        return reply(@{@"ok": @YES, @"bundle_id": app.bundleIdentifier ?: @"",
                       @"app_name": app.localizedName ?: @"", @"elements": tree, @"truncated": @(budget == 0)});
    }
}

static CGEventFlags flagsFor(NSArray *modifiers) {
    CGEventFlags flags = 0;
    for (NSString *modifier in modifiers) {
        if ([modifier isEqualToString:@"command"]) flags |= kCGEventFlagMaskCommand;
        if ([modifier isEqualToString:@"shift"]) flags |= kCGEventFlagMaskShift;
        if ([modifier isEqualToString:@"control"]) flags |= kCGEventFlagMaskControl;
        if ([modifier isEqualToString:@"option"]) flags |= kCGEventFlagMaskAlternate;
    }
    return flags;
}

static int codeForKey(NSString *key) {
    // Hardware key codes for common macOS navigation and shortcut keys.
    static NSDictionary *codes;
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        codes = @{@"a": @0, @"b": @11, @"c": @8, @"d": @2, @"e": @14, @"f": @3,
            @"g": @5, @"h": @4, @"i": @34, @"j": @38, @"k": @40, @"l": @37,
            @"m": @46, @"n": @45, @"o": @31, @"p": @35, @"q": @12, @"r": @15,
            @"s": @1, @"t": @17, @"u": @32, @"v": @9, @"w": @13, @"x": @7,
            @"y": @16, @"z": @6, @"0": @29, @"1": @18, @"2": @19, @"3": @20,
            @"4": @21, @"5": @23, @"6": @22, @"7": @26, @"8": @28, @"9": @25,
            @"enter": @36, @"return": @36, @"escape": @53, @"tab": @48, @"space": @49,
            @"backspace": @51, @"delete": @117, @"up": @126, @"down": @125,
            @"left": @123, @"right": @124, @"home": @115, @"end": @119};
    });
    NSNumber *number = codes[key.lowercaseString];
    return number ? number.intValue : -1;
}

static void postMouse(CGEventType type, CGPoint point, CGMouseButton button) {
    CGEventRef event = CGEventCreateMouseEvent(NULL, type, point, button);
    if (event) { CGEventPost(kCGHIDEventTap, event); CFRelease(event); }
}

char *computer_native_action(const char *request) {
    @autoreleasepool {
        if (isScreenLocked()) return failure(@"SCREEN_LOCKED", @"Unlock the desktop before controlling applications");
        if (!AXIsProcessTrusted()) return failure(@"ACCESSIBILITY_PERMISSION_REQUIRED", @"Enable Accessibility permission in macOS Settings");
        NSData *bytes = [NSData dataWithBytes:request length:strlen(request)];
        NSDictionary *args = [NSJSONSerialization JSONObjectWithData:bytes options:0 error:nil];
        if (![args isKindOfClass:NSDictionary.class]) return failure(@"INVALID_ARGUMENT", @"Invalid input action");
        NSString *action = args[@"action"];
        NSRunningApplication *app = NSWorkspace.sharedWorkspace.frontmostApplication;
        NSString *target = args[@"bundle_id"];
        if ([action isEqualToString:@"focus_app"]) {
            for (NSRunningApplication *candidate in NSWorkspace.sharedWorkspace.runningApplications) {
                if ([candidate.bundleIdentifier isEqualToString:target] &&
                    candidate.activationPolicy == NSApplicationActivationPolicyRegular) {
                    BOOL activated = [candidate activateWithOptions:NSApplicationActivateAllWindows];
                    if (!activated) return failure(@"APP_ACTIVATION_FAILED", @"Unable to activate selected application");
                    return reply(@{@"ok": @YES, @"action": action, @"bundle_id": target});
                }
            }
            return failure(@"APP_NOT_FOUND", @"Approved application is not running");
        }
        if ([action isEqualToString:@"open_url"]) {
            // Local CONTROL already limits the target bundle. This operation
            // may only navigate an explicitly supported browser, never an
            // arbitrary application, shell handler, file or custom scheme.
            NSSet *browsers = [NSSet setWithArray:@[
                @"com.microsoft.edgemac", @"com.google.Chrome", @"com.apple.Safari",
                @"org.mozilla.firefox", @"com.brave.Browser", @"company.thebrowser.Browser",
                @"com.vivaldi.Vivaldi", @"com.operasoftware.Opera"
            ]];
            if (![browsers containsObject:target])
                return failure(@"BROWSER_ONLY", @"open_url requires locally approved browser control");
            NSString *rawURL = args[@"url"];
            if (![rawURL isKindOfClass:NSString.class] || rawURL.length == 0 ||
                rawURL.length > 2048 || [rawURL rangeOfCharacterFromSet:
                    NSCharacterSet.whitespaceAndNewlineCharacterSet].location != NSNotFound)
                return failure(@"INVALID_BROWSER_URL", @"Expected a short HTTP(S) URL without whitespace");
            NSURLComponents *parts = [NSURLComponents componentsWithString:rawURL];
            if (!parts || ![@[@"https", @"http"] containsObject:parts.scheme.lowercaseString] ||
                parts.host.length == 0 || parts.user.length > 0 || parts.password.length > 0)
                return failure(@"INVALID_BROWSER_URL", @"Only HTTP(S) URLs without embedded credentials are accepted");
            NSURL *url = parts.URL;
            if (!url) return failure(@"INVALID_BROWSER_URL", @"Invalid URL");
            NSRunningApplication *browser = nil;
            for (NSRunningApplication *candidate in NSWorkspace.sharedWorkspace.runningApplications) {
                if ([candidate.bundleIdentifier isEqualToString:target] &&
                    candidate.activationPolicy == NSApplicationActivationPolicyRegular) {
                    browser = candidate;
                    break;
                }
            }
            if (!browser.bundleURL)
                return failure(@"BROWSER_NOT_RUNNING", @"The approved browser is not running");
            dispatch_semaphore_t completion = dispatch_semaphore_create(0);
            __block NSError *openError = nil;
            NSWorkspaceOpenConfiguration *configuration = [NSWorkspaceOpenConfiguration configuration];
            configuration.activates = YES;
            [NSWorkspace.sharedWorkspace openURLs:@[url]
                             withApplicationAtURL:browser.bundleURL
                                   configuration:configuration
                               completionHandler:^(NSRunningApplication *openedApp, NSError *error) {
                (void)openedApp;
                openError = error;
                dispatch_semaphore_signal(completion);
            }];
            if (dispatch_semaphore_wait(completion, dispatch_time(DISPATCH_TIME_NOW, 6 * NSEC_PER_SEC)) != 0)
                return failure(@"BROWSER_OPEN_TIMEOUT", @"Browser did not acknowledge URL navigation");
            if (openError)
                return failure(@"BROWSER_OPEN_FAILED", openError.localizedDescription ?: @"Browser refused URL");
            return reply(@{@"ok": @YES, @"action": @"open_url", @"bundle_id": target,
                           @"dispatched": @YES});
        }
        if (!app || !target || ![app.bundleIdentifier isEqualToString:target])
            return failure(@"FOREGROUND_APP_CHANGED", @"Foreground app differs from the one approved locally");
        if ([action isEqualToString:@"press_element"]) {
            NSString *label = args[@"label"];
            NSString *role = args[@"role"];
            if (![label isKindOfClass:NSString.class] || label.length == 0 || label.length > 160)
                return failure(@"INVALID_ARGUMENT", @"Element label must be 1–160 characters");
            if (role && ![role isKindOfClass:NSString.class]) return failure(@"INVALID_ARGUMENT", @"Invalid element role");
            // Explicit high-risk approval is supplied ONLY by the Rust lease
            // manager after consuming a one-time locally approved permit.
            BOOL confirmed = [args[@"high_risk_confirmed"] boolValue];
            NSString *normalized = label.lowercaseString;
            for (NSString *dangerous in @[@"delete", @"remove", @"pay", @"purchase", @"submit", @"send",
                                           @"erase", @"transfer", @"删除", @"支付", @"付款", @"购买", @"提交",
                                           @"发送", @"转账", @"移除", @"清空", @"确认订单", @"卸载", @"永久"]) {
                if ([normalized containsString:dangerous] && !confirmed)
                    return failure(@"CONFIRMATION_REQUIRED", @"Potentially destructive or financial AX actions are not enabled");
            }
            AXUIElementRef root = AXUIElementCreateApplication(app.processIdentifier);
            if (!root) return failure(@"AX_UNAVAILABLE", @"Cannot read current application controls");
            AXUIElementSetMessagingTimeout(root, 1.0);
            NSUInteger budget = 200;
            NSMutableArray *matches = [NSMutableArray array];
            findPressTargets(root, label, role ?: @"", 0, &budget, matches);
            CFRelease(root);
            if (matches.count != 1) return failure(@"ELEMENT_NOT_UNIQUE", @"Element not found or not unique; refresh UI tree first");
            AXError result = AXUIElementPerformAction((__bridge AXUIElementRef)matches.firstObject, kAXPressAction);
            if (result != kAXErrorSuccess) return failure(@"AX_PRESS_FAILED", @"The selected control does not support AXPress");
        } else if ([action isEqualToString:@"type"]) {
            NSString *value = args[@"text"];
            if (![value isKindOfClass:NSString.class] || value.length > 2000)
                return failure(@"INVALID_ARGUMENT", @"Text must be under 2000 UTF-16 characters");
            if (isSecureFocusedField(app.processIdentifier))
                return failure(@"SECURE_INPUT_REJECTED", @"Cannot type into a secure password field");
            // Some Chromium text fields silently drop a single synthetic
            // event containing the entire string. Emit each composed Unicode
            // character as its own key down/up inside ONE native invocation.
            // This preserves emoji/surrogate pairs and avoids MCP roundtrips
            // per character. The secure-field check above remains mandatory.
            __block BOOL failed = NO;
            [value enumerateSubstringsInRange:NSMakeRange(0, value.length)
                options:NSStringEnumerationByComposedCharacterSequences
                usingBlock:^(NSString *substring, NSRange range, NSRange enclosingRange, BOOL *stop) {
                    (void)range; (void)enclosingRange;
                    CGEventRef down = CGEventCreateKeyboardEvent(NULL, 0, true);
                    CGEventRef up = CGEventCreateKeyboardEvent(NULL, 0, false);
                    if (!down || !up) {
                        if (down) CFRelease(down);
                        if (up) CFRelease(up);
                        failed = YES;
                        *stop = YES;
                        return;
                    }
                    UniChar units[16];
                    if (substring.length > 16) {
                        failed = YES;
                        CFRelease(down); CFRelease(up);
                        *stop = YES;
                        return;
                    }
                    [substring getCharacters:units range:NSMakeRange(0, substring.length)];
                    CGEventKeyboardSetUnicodeString(down, substring.length, units);
                    CGEventKeyboardSetUnicodeString(up, substring.length, units);
                    CGEventPost(kCGHIDEventTap, down);
                    CGEventPost(kCGHIDEventTap, up);
                    CFRelease(down); CFRelease(up);
                }];
            if (failed) return failure(@"EVENT_CREATE_FAILED", @"Unable to synthesize Unicode character");
        } else if ([action isEqualToString:@"key"]) {
            if (isSecureFocusedField(app.processIdentifier))
                return failure(@"SECURE_INPUT_REJECTED", @"Keyboard actions are disabled for secure or inaccessible focused fields");
            NSString *key = args[@"key"];
            int keyCode = codeForKey(key ?: @"");
            if (keyCode < 0) return failure(@"UNSUPPORTED_KEY", @"Unsupported key code");
            NSArray *mods = args[@"modifiers"];
            if (![mods isKindOfClass:NSArray.class]) mods = @[];
            CGEventFlags flags = flagsFor(mods);
            BOOL confirmed = [args[@"high_risk_confirmed"] boolValue];
            if (flags & kCGEventFlagMaskCommand &&
                [@[@"q", @"w", @"v", @"delete", @"backspace"] containsObject:key.lowercaseString] && !confirmed)
                return failure(@"CONFIRMATION_REQUIRED", @"This shortcut needs a one-time local confirmation");
            if (flags & kCGEventFlagMaskControl &&
                [@[@"delete", @"backspace"] containsObject:key.lowercaseString] && !confirmed)
                return failure(@"CONFIRMATION_REQUIRED", @"This control shortcut needs a one-time local confirmation");
            CGEventRef down = CGEventCreateKeyboardEvent(NULL, keyCode, true);
            CGEventRef up = CGEventCreateKeyboardEvent(NULL, keyCode, false);
            if (!down || !up) { if (down) CFRelease(down); if (up) CFRelease(up); return failure(@"EVENT_CREATE_FAILED", @"Cannot create key event"); }
            CGEventSetFlags(down, flags); CGEventSetFlags(up, flags);
            CGEventPost(kCGHIDEventTap, down); CGEventPost(kCGHIDEventTap, up);
            CFRelease(down); CFRelease(up);
        } else if ([action isEqualToString:@"scroll"]) {
            int amount = [args[@"amount"] intValue];
            if (amount < -15 || amount > 15) return failure(@"INVALID_ARGUMENT", @"Scroll amount exceeds limit");
            CGEventRef event = CGEventCreateScrollWheelEvent(NULL, kCGScrollEventUnitLine, 1, amount);
            if (event) { CGEventPost(kCGHIDEventTap, event); CFRelease(event); }
        } else if ([@[@"click", @"double_click", @"right_click", @"move", @"drag"] containsObject:action]) {
            double x = [args[@"x"] doubleValue], y = [args[@"y"] doubleValue];
            if (!isfinite(x) || !isfinite(y)) return failure(@"INVALID_COORDINATE", @"Coordinates must be finite");
            CGRect bounds = CGRectNull;
            uint32_t displayCount = 0;
            CGDirectDisplayID displays[16] = {0};
            CGGetActiveDisplayList(16, displays, &displayCount);
            for (uint32_t i = 0; i < displayCount; i++) {
                CGRect frame = CGDisplayBounds(displays[i]);
                if (CGRectContainsPoint(frame, CGPointMake(x, y))) { bounds = frame; break; }
            }
            if (CGRectIsNull(bounds)) return failure(@"OUTSIDE_SCREEN", @"Coordinate is outside active displays");
            CGPoint location = CGPointMake(x, y);
            if (!pointHitsApprovedWindow(location, app.processIdentifier))
                return failure(@"TARGET_WINDOW_MISMATCH", @"Pointer coordinate does not hit an unobstructed window of the locally approved application");
            if ([action isEqualToString:@"move"]) {
                postMouse(kCGEventMouseMoved, location, kCGMouseButtonLeft);
            } else if ([action isEqualToString:@"drag"]) {
                double endX = [args[@"end_x"] doubleValue], endY = [args[@"end_y"] doubleValue];
                BOOL targetInside = NO;
                for (uint32_t i = 0; i < displayCount; i++) {
                    if (CGRectContainsPoint(CGDisplayBounds(displays[i]), CGPointMake(endX, endY))) targetInside = YES;
                }
                if (!isfinite(endX) || !isfinite(endY) || !targetInside)
                    return failure(@"OUTSIDE_SCREEN", @"Drag endpoint is outside active displays");
                for (int step = 0; step <= 10; step++) {
                    CGPoint next = CGPointMake(x + (endX - x) * step / 10.0, y + (endY - y) * step / 10.0);
                    if (!pointHitsApprovedWindow(next, app.processIdentifier))
                        return failure(@"TARGET_WINDOW_MISMATCH", @"Drag path crosses another application's window or a protected overlay");
                }
                postMouse(kCGEventLeftMouseDown, location, kCGMouseButtonLeft);
                for (int step = 1; step <= 10; step++) {
                    CGPoint next = CGPointMake(x + (endX - x) * step / 10.0, y + (endY - y) * step / 10.0);
                    postMouse(kCGEventLeftMouseDragged, next, kCGMouseButtonLeft);
                }
                postMouse(kCGEventLeftMouseUp, CGPointMake(endX, endY), kCGMouseButtonLeft);
            } else {
                BOOL right = [action isEqualToString:@"right_click"];
                CGEventType down = right ? kCGEventRightMouseDown : kCGEventLeftMouseDown;
                CGEventType up = right ? kCGEventRightMouseUp : kCGEventLeftMouseUp;
                CGMouseButton button = right ? kCGMouseButtonRight : kCGMouseButtonLeft;
                int clicks = [action isEqualToString:@"double_click"] ? 2 : 1;
                for (int n = 1; n <= clicks; n++) {
                    CGEventRef downEvent = CGEventCreateMouseEvent(NULL, down, location, button);
                    CGEventRef upEvent = CGEventCreateMouseEvent(NULL, up, location, button);
                    if (!downEvent || !upEvent) { if (downEvent) CFRelease(downEvent); if (upEvent) CFRelease(upEvent); return failure(@"EVENT_CREATE_FAILED", @"Cannot create mouse event"); }
                    CGEventSetIntegerValueField(downEvent, kCGMouseEventClickState, n);
                    CGEventSetIntegerValueField(upEvent, kCGMouseEventClickState, n);
                    CGEventPost(kCGHIDEventTap, downEvent); CGEventPost(kCGHIDEventTap, upEvent);
                    CFRelease(downEvent); CFRelease(upEvent);
                }
            }
        } else {
            return failure(@"INVALID_ACTION", @"Unsupported computer action");
        }
        return reply(@{@"ok": @YES, @"action": action ?: @""});
    }
}

static char *failure(NSString *code, NSString *details) {
    return reply(@{@"ok": @NO, @"error": code ?: @"NATIVE_ERROR",
                   @"message": details ?: @"Native operation failed"});
}

void computer_native_free(char *p) { free(p); }

// Screen Recording preflight is a diagnostic, not an authority boundary:
// SCScreenshotManager and SCShareableContent enforce macOS TCC themselves.
// Calling these requires a separate explicitly approved, scoped MCP lease
// checked by the Rust gateway before entering this native bridge.

static BOOL isScreenLocked(void) {
    CFDictionaryRef session = CGSessionCopyCurrentDictionary();
    if (!session) return YES;
    NSDictionary *status = (__bridge NSDictionary *)session;
    BOOL locked = [status[@"CGSSessionScreenIsLocked"] boolValue] ||
                  (status[(__bridge NSString *)kCGSessionOnConsoleKey] &&
                  ![status[(__bridge NSString *)kCGSessionOnConsoleKey] boolValue]);
    CFRelease(session);
    return locked;
}

// CGWindowListCopyWindowInfo returns windows in front-to-back order. Only
// deliver pointer events to a visible window owned by the app selected locally
// for this grant. An unrelated overlay/window above it is a hard deny.
static BOOL pointHitsApprovedWindow(CGPoint point, pid_t approvedPID) {
    CFArrayRef windowList = CGWindowListCopyWindowInfo(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        kCGNullWindowID);
    if (!windowList) return NO;
    BOOL approved = NO;
    for (NSDictionary *window in (__bridge NSArray *)windowList) {
        NSNumber *layer = window[(__bridge NSString *)kCGWindowLayer];
        if (!layer) continue;
        NSDictionary *rect = window[(__bridge NSString *)kCGWindowBounds];
        if (![rect isKindOfClass:NSDictionary.class]) continue;
        CGRect bounds = CGRectZero;
        if (!CGRectMakeWithDictionaryRepresentation((__bridge CFDictionaryRef)rect, &bounds) ||
            !CGRectContainsPoint(bounds, point)) continue;
        // A surface above the app, including an overlay, must never be clicked
        // under another app's authorization.
        approved = layer.integerValue == 0 &&
            [window[(__bridge NSString *)kCGWindowOwnerPID] intValue] == approvedPID;
        break;
    }
    CFRelease(windowList);
    return approved;
}

static SCShareableContent *shareableContent(NSError **failureOut) {
    dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
    __block SCShareableContent *result = nil;
    __block NSError *resultError = nil;
    [SCShareableContent getShareableContentExcludingDesktopWindows:NO
                                           onScreenWindowsOnly:NO
                                             completionHandler:^(SCShareableContent *content, NSError *error) {
        result = content;
        resultError = error;
        dispatch_semaphore_signal(semaphore);
    }];
    if (dispatch_semaphore_wait(semaphore, dispatch_time(DISPATCH_TIME_NOW, 7 * NSEC_PER_SEC)) != 0) {
        if (failureOut) *failureOut = [NSError errorWithDomain:@"ComputerUse" code:1
            userInfo:@{NSLocalizedDescriptionKey: @"ScreenCaptureKit content discovery timed out"}];
        return nil;
    }
    if (failureOut) *failureOut = resultError;
    return result;
}

// Only window metadata (not content) is returned, and window title may contain
// private data. The Rust layer must require an approved observe session.
char *computer_native_windows(void) {
    @autoreleasepool {
        if (@available(macOS 14.0, *)) {
            if (isScreenLocked()) return failure(@"SCREEN_LOCKED", @"Unlock the current macOS desktop before observing");
            NSError *error = nil;
            SCShareableContent *content = shareableContent(&error);
            if (!content) return failure(@"SCREEN_DISCOVERY_FAILED", error.localizedDescription ?: @"ScreenCaptureKit cannot enumerate screens; verify macOS Screen Recording permission and app signature");
            NSMutableArray *displays = [NSMutableArray array];
            for (SCDisplay *d in content.displays) {
                CGRect frame = d.frame;
                [displays addObject:@{@"id": @(d.displayID), @"width": @(d.width), @"height": @(d.height),
                    @"x": @(frame.origin.x), @"y": @(frame.origin.y),
                    @"point_width": @(frame.size.width), @"point_height": @(frame.size.height)}];
            }
            NSMutableArray *windows = [NSMutableArray array];
            for (SCWindow *w in content.windows) {
                if (windows.count >= 100) break;
                CGRect f = w.frame;
                [windows addObject:@{@"id": @(w.windowID),
                    @"app": w.owningApplication.applicationName ?: @"",
                    @"bundle_id": w.owningApplication.bundleIdentifier ?: @"",
                    @"on_screen": @(w.isOnScreen),
                    @"layer": @(w.windowLayer),
                    @"title": w.title ?: @"",
                    @"x": @(f.origin.x), @"y": @(f.origin.y),
                    @"width": @(f.size.width), @"height": @(f.size.height)}];
            }
            return reply(@{@"ok": @YES, @"displays": displays, @"windows": windows});
        }
        return failure(@"UNSUPPORTED_MACOS", @"ScreenCaptureKit screenshots require macOS 14 or later");
    }
}

char *computer_native_capture(const char *request) {
    @autoreleasepool {
        if (@available(macOS 14.0, *)) {
            if (isScreenLocked()) return failure(@"SCREEN_LOCKED", @"Unlock the current macOS desktop before capturing");
            NSData *bytes = [NSData dataWithBytes:request length:strlen(request)];
            NSDictionary *args = [NSJSONSerialization JSONObjectWithData:bytes options:0 error:nil];
            if (![args isKindOfClass:NSDictionary.class]) return failure(@"INVALID_ARGUMENT", @"Invalid capture arguments");
            uint32_t displayID = [args[@"display_id"] unsignedIntValue];
            uint32_t windowID = [args[@"window_id"] unsignedIntValue];
            NSString *expectedBundle = args[@"expected_bundle_id"];
            if (![expectedBundle isKindOfClass:NSString.class]) expectedBundle = @"";
            if (expectedBundle.length > 0 && windowID == 0)
                return failure(@"TARGET_WINDOW_REQUIRED", @"Control grants can capture only windows of the approved application; select window_id from computer_observe(windows)");
            NSError *error = nil;
            SCShareableContent *content = shareableContent(&error);
            if (!content) return failure(@"SCREEN_DISCOVERY_FAILED", error.localizedDescription ?: @"ScreenCaptureKit denied discovery; verify macOS Screen Recording permission and app signature");
            SCContentFilter *filter = nil;
            CGRect logical = CGRectZero;
            if (windowID != 0) {
                for (SCWindow *window in content.windows) {
                    if (window.windowID == windowID) {
                        if (expectedBundle.length > 0 &&
                            ![window.owningApplication.bundleIdentifier isEqualToString:expectedBundle])
                            return failure(@"TARGET_WINDOW_MISMATCH", @"Selected window does not belong to the locally approved app");
                        filter = [[SCContentFilter alloc] initWithDesktopIndependentWindow:window];
                        logical = window.frame;
                        break;
                    }
                }
                if (!filter) return failure(@"WINDOW_NOT_FOUND", @"Window is not available for capture");
            } else {
                SCDisplay *display = nil;
                for (SCDisplay *candidate in content.displays) {
                    if (candidate.displayID == displayID) { display = candidate; break; }
                }
                if (!display) return failure(@"DISPLAY_NOT_FOUND", @"Display is not available for capture");
                filter = [[SCContentFilter alloc] initWithDisplay:display excludingWindows:@[]];
                logical = display.frame;
            }
            if (logical.size.width <= 0 || logical.size.height <= 0) return failure(@"INVALID_FRAME", @"Invalid frame geometry");
            CGRect capture = logical;
            NSDictionary *area = args[@"region"];
            if ([area isKindOfClass:NSDictionary.class]) {
                // ScreenCaptureKit ignores sourceRect for a single-window
                // filter; rejecting this prevents incorrect click coordinates.
                if (windowID != 0)
                    return failure(@"UNSUPPORTED_REGION", @"Window and region capture cannot be combined");
                capture = CGRectMake([area[@"x"] doubleValue], [area[@"y"] doubleValue],
                                    [area[@"width"] doubleValue], [area[@"height"] doubleValue]);
                if (!isfinite(capture.origin.x) || !isfinite(capture.origin.y) ||
                    !isfinite(capture.size.width) || !isfinite(capture.size.height) ||
                    capture.size.width <= 0 || capture.size.height <= 0 ||
                    !CGRectContainsRect(logical, capture))
                    return failure(@"INVALID_REGION", @"Capture region must be inside the selected display/window");
            }
            // Screen points → captured pixels. Limit dimensions before capture,
            // and expose precise output pixel-to-global-point ratios to callers.
            CGFloat nativeScale = MAX(1.0, filter.pointPixelScale);
            double requestedW = capture.size.width * nativeScale;
            double requestedH = capture.size.height * nativeScale;
            NSString *quality = args[@"quality"];
            if (quality && (![quality isKindOfClass:NSString.class] ||
                            ![@[@"fast", @"detailed"] containsObject:quality]))
                return failure(@"INVALID_QUALITY", @"Supported screenshot quality: fast, detailed");
            BOOL detailed = [quality isEqualToString:@"detailed"];
            double maxWidth = detailed ? 1920.0 : 1280.0;
            double maxHeight = detailed ? 1200.0 : 850.0;
            double shrink = MIN(1.0, MIN(maxWidth / requestedW, maxHeight / requestedH));
            SCStreamConfiguration *config = [SCStreamConfiguration new];
            config.width = MAX(1, (size_t)round(requestedW * shrink));
            config.height = MAX(1, (size_t)round(requestedH * shrink));
            config.showsCursor = YES;
            if ([area isKindOfClass:NSDictionary.class]) {
                config.sourceRect = CGRectMake(capture.origin.x - logical.origin.x,
                    capture.origin.y - logical.origin.y, capture.size.width, capture.size.height);
            }
            dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
            __block CGImageRef captured = NULL;
            __block NSError *capturedError = nil;
            [SCScreenshotManager captureImageWithFilter:filter configuration:config
                completionHandler:^(CGImageRef image, NSError *callbackError) {
                    if (image) captured = CGImageRetain(image);
                    capturedError = callbackError;
                    dispatch_semaphore_signal(semaphore);
                }];
            if (dispatch_semaphore_wait(semaphore, dispatch_time(DISPATCH_TIME_NOW, 8 * NSEC_PER_SEC)) != 0)
                return failure(@"SCREEN_CAPTURE_TIMEOUT", @"Native capture timed out");
            if (!captured) return failure(@"SCREEN_CAPTURE_FAILED", capturedError.localizedDescription ?: @"ScreenCaptureKit did not return an image; check macOS Screen Recording permission and app signature");
            NSBitmapImageRep *bitmap = [[NSBitmapImageRep alloc] initWithCGImage:captured];
            size_t width = CGImageGetWidth(captured), height = CGImageGetHeight(captured);
            CGImageRelease(captured);
            if (width == 0 || height == 0) return failure(@"EMPTY_SCREENSHOT", @"Native capture returned an empty frame");
            NSData *jpeg = [bitmap representationUsingType:NSBitmapImageFileTypeJPEG
                                                properties:@{NSImageCompressionFactor: @(detailed ? 0.72 : 0.56)}];
            if (!jpeg || jpeg.length > 8 * 1024 * 1024) return failure(@"SCREEN_IMAGE_TOO_LARGE", @"JPEG screenshot exceeds 8 MiB");
            return reply(@{@"ok": @YES, @"mime_type": @"image/jpeg",
                @"base64": [jpeg base64EncodedStringWithOptions:0],
                @"pixel_width": @(width), @"pixel_height": @(height),
                @"origin_x": @(capture.origin.x), @"origin_y": @(capture.origin.y),
                @"point_width": @(capture.size.width), @"point_height": @(capture.size.height),
                @"point_per_pixel_x": @(capture.size.width / width),
                @"point_per_pixel_y": @(capture.size.height / height),
                @"display_id": @(displayID), @"window_id": @(windowID)});
        }
        return failure(@"UNSUPPORTED_MACOS", @"ScreenCaptureKit screenshots require macOS 14 or later");
    }
}
