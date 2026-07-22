#import "BackgroundReplayScheduler.h"

#import <BackgroundTasks/BackgroundTasks.h>

static NSString *const StarlaneBackgroundReplayWakeReason = @"apple-bg-task";
static NSString *const StarlaneBackgroundReplayDefaultsSuite = @"starlane_background_replay_worker";
static NSString *const StarlaneBackgroundReplayReceiptSource = @"ns-user-defaults";
static NSTimeInterval const StarlaneBackgroundReplayMinimumInterval = 15 * 60;

@implementation StarlaneBackgroundReplayScheduler

+ (void)registerDefaultSchedules {
  if (@available(iOS 13.0, *)) {
    for (NSString *provider in @[ @"none", @"icloud-drive", @"google-drive" ]) {
      [self registerProviderSchedule:provider minIntervalSeconds:StarlaneBackgroundReplayMinimumInterval];
    }
  }
}

+ (NSString *)taskIdentifierForProvider:(NSString *)provider {
  NSString *sanitized = [self sanitizedProvider:provider];
  return [NSString stringWithFormat:@"dev.starlane.notes.background-replay.%@", sanitized];
}

+ (void)registerProviderSchedule:(NSString *)provider minIntervalSeconds:(NSTimeInterval)minIntervalSeconds API_AVAILABLE(ios(13.0)) {
  NSString *taskIdentifier = [self taskIdentifierForProvider:provider];
  BOOL registered = [[BGTaskScheduler sharedScheduler]
      registerForTaskWithIdentifier:taskIdentifier
                         usingQueue:nil
                      launchHandler:^(__kindof BGTask *task) {
                        [self handleTask:task provider:provider minIntervalSeconds:minIntervalSeconds];
                      }];

  [self recordRegistrationForProvider:provider taskIdentifier:taskIdentifier registered:registered];
  [self scheduleProvider:provider minIntervalSeconds:minIntervalSeconds];
}

+ (void)scheduleProvider:(NSString *)provider minIntervalSeconds:(NSTimeInterval)minIntervalSeconds API_AVAILABLE(ios(13.0)) {
  NSString *taskIdentifier = [self taskIdentifierForProvider:provider];
  BGProcessingTaskRequest *request = [[BGProcessingTaskRequest alloc] initWithIdentifier:taskIdentifier];
  request.requiresNetworkConnectivity = YES;
  request.requiresExternalPower = NO;
  request.earliestBeginDate = [NSDate dateWithTimeIntervalSinceNow:MAX(minIntervalSeconds, StarlaneBackgroundReplayMinimumInterval)];

  NSError *error = nil;
  BOOL submitted = [[BGTaskScheduler sharedScheduler] submitTaskRequest:request error:&error];
  [self recordSubmissionForProvider:provider taskIdentifier:taskIdentifier submitted:submitted error:error];
}

+ (void)handleTask:(BGTask *)task provider:(NSString *)provider minIntervalSeconds:(NSTimeInterval)minIntervalSeconds API_AVAILABLE(ios(13.0)) {
  __block BOOL expired = NO;
  task.expirationHandler = ^{
    expired = YES;
  };

  [self recordWakeForProvider:provider taskIdentifier:task.identifier expired:expired];
  [self scheduleProvider:provider minIntervalSeconds:minIntervalSeconds];
  [task setTaskCompletedWithSuccess:!expired];
}

+ (void)recordRegistrationForProvider:(NSString *)provider taskIdentifier:(NSString *)taskIdentifier registered:(BOOL)registered {
  NSUserDefaults *defaults = [self replayDefaults];
  [defaults setObject:provider forKey:@"last_provider"];
  [defaults setObject:taskIdentifier forKey:@"last_task_identifier"];
  [defaults setBool:registered forKey:@"last_registration_succeeded"];
  [defaults setDouble:[[NSDate date] timeIntervalSince1970] * 1000 forKey:@"last_registration_epoch_millis"];
  [defaults synchronize];
}

+ (void)recordSubmissionForProvider:(NSString *)provider taskIdentifier:(NSString *)taskIdentifier submitted:(BOOL)submitted error:(NSError *)error {
  NSUserDefaults *defaults = [self replayDefaults];
  [defaults setObject:provider forKey:@"last_provider"];
  [defaults setObject:taskIdentifier forKey:@"last_task_identifier"];
  [defaults setBool:submitted forKey:@"last_submission_succeeded"];
  [defaults setDouble:[[NSDate date] timeIntervalSince1970] * 1000 forKey:@"last_submission_epoch_millis"];
  if (error != nil) {
    [defaults setObject:error.localizedDescription forKey:@"last_submission_error"];
  } else {
    [defaults removeObjectForKey:@"last_submission_error"];
  }
  [defaults synchronize];
}

+ (void)recordWakeForProvider:(NSString *)provider taskIdentifier:(NSString *)taskIdentifier expired:(BOOL)expired {
  NSUserDefaults *defaults = [self replayDefaults];
  [defaults setObject:provider forKey:@"last_provider"];
  [defaults setObject:StarlaneBackgroundReplayWakeReason forKey:@"last_wake_reason"];
  [defaults setObject:taskIdentifier forKey:@"last_task_identifier"];
  [defaults setObject:StarlaneBackgroundReplayReceiptSource forKey:@"last_wake_source"];
  [defaults setBool:expired forKey:@"last_wake_expired"];
  [defaults setDouble:[[NSDate date] timeIntervalSince1970] * 1000 forKey:@"last_wake_epoch_millis"];
  [defaults synchronize];
}

+ (NSUserDefaults *)replayDefaults {
  NSUserDefaults *defaults = [[NSUserDefaults alloc] initWithSuiteName:StarlaneBackgroundReplayDefaultsSuite];
  return defaults ?: NSUserDefaults.standardUserDefaults;
}

+ (NSString *)sanitizedProvider:(NSString *)provider {
  NSCharacterSet *allowed = [NSCharacterSet characterSetWithCharactersInString:@"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_"];
  NSMutableString *result = [NSMutableString string];
  for (NSUInteger index = 0; index < provider.length; index += 1) {
    unichar character = [provider characterAtIndex:index];
    if ([allowed characterIsMember:character]) {
      [result appendFormat:@"%C", character];
    } else {
      [result appendString:@"_"];
    }
  }

  NSString *trimmed = [result stringByTrimmingCharactersInSet:[NSCharacterSet characterSetWithCharactersInString:@"_"]];
  return trimmed.length > 0 ? trimmed : @"default";
}

@end
