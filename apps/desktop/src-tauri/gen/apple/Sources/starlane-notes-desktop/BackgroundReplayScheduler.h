#import <Foundation/Foundation.h>

NS_ASSUME_NONNULL_BEGIN

@interface StarlaneBackgroundReplayScheduler : NSObject

+ (void)registerDefaultSchedules;
+ (NSString *)taskIdentifierForProvider:(NSString *)provider;

@end

NS_ASSUME_NONNULL_END
