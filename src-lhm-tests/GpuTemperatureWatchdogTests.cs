namespace OndoHardwareMonitor.Tests;

public class GpuTemperatureWatchdogTests
{
    const int First = GpuTemperatureWatchdog.FirstResetAfterTicks;

    /// Feeds `ticks` ticks of a present GPU without a temperature and returns
    /// the 1-based ticks on which a rebuild was requested.
    static List<int> MissingTicks(GpuTemperatureWatchdog watchdog, int ticks)
    {
        var resets = new List<int>();
        for (int i = 1; i <= ticks; i++)
        {
            if (watchdog.Observe(gpuPresent: true, hasTemperature: false))
                resets.Add(i);
        }
        return resets;
    }

    [Fact]
    public void NeverResetsWhileTheTemperatureIsReported()
    {
        var watchdog = new GpuTemperatureWatchdog();

        for (int i = 0; i < First * 100; i++)
            Assert.False(watchdog.Observe(gpuPresent: true, hasTemperature: true));
    }

    [Fact]
    public void ResetsAfterTheFirstRunOfMissingTicks()
    {
        var watchdog = new GpuTemperatureWatchdog();

        Assert.Equal([First], MissingTicks(watchdog, First));
        Assert.Equal(1, watchdog.Resets);
    }

    [Fact]
    public void DoublesTheWaitAndStopsAfterMaxResets()
    {
        var watchdog = new GpuTemperatureWatchdog();

        var expected = new List<int>();
        var tick = 0;
        for (int n = 0; n < GpuTemperatureWatchdog.MaxResets; n++)
        {
            tick += First << n;
            expected.Add(tick);
        }

        // Far past the last rebuild: nothing more once the budget is spent
        Assert.Equal(expected, MissingTicks(watchdog, tick * 4));
        Assert.Equal(GpuTemperatureWatchdog.MaxResets, watchdog.Resets);
    }

    [Fact]
    public void ATemperatureReadingClearsTheBackoff()
    {
        var watchdog = new GpuTemperatureWatchdog();
        MissingTicks(watchdog, First * (1 + 2 + 4));
        Assert.Equal(3, watchdog.Resets);

        watchdog.Observe(gpuPresent: true, hasTemperature: true);

        Assert.Equal(0, watchdog.Resets);
        Assert.Equal([First], MissingTicks(watchdog, First));
    }

    [Fact]
    public void ATemperatureReadingRestartsTheCount()
    {
        var watchdog = new GpuTemperatureWatchdog();
        MissingTicks(watchdog, First - 1);

        watchdog.Observe(gpuPresent: true, hasTemperature: true);

        Assert.Equal([First], MissingTicks(watchdog, First));
    }

    [Fact]
    public void RemembersWhetherTheGpuEverHadATemperature()
    {
        var watchdog = new GpuTemperatureWatchdog();
        MissingTicks(watchdog, First);
        Assert.False(watchdog.HadTemperature);

        watchdog.Observe(gpuPresent: true, hasTemperature: true);
        MissingTicks(watchdog, First);

        Assert.True(watchdog.HadTemperature);
    }

    [Fact]
    public void AMissingGpuDoesNotCountAsAMissingTemperature()
    {
        var watchdog = new GpuTemperatureWatchdog();

        for (int i = 0; i < First * 10; i++)
            Assert.False(watchdog.Observe(gpuPresent: false, hasTemperature: false));
        Assert.Equal(0, watchdog.Resets);
    }

    [Fact]
    public void AMissingGpuRestartsTheCountButKeepsTheBackoff()
    {
        var watchdog = new GpuTemperatureWatchdog();
        MissingTicks(watchdog, First + (First * 2 - 1));
        Assert.Equal(1, watchdog.Resets);

        watchdog.Observe(gpuPresent: false, hasTemperature: false);

        // Second rebuild still waits the doubled run, counted from scratch
        Assert.Equal([First * 2], MissingTicks(watchdog, First * 2));
        Assert.Equal(2, watchdog.Resets);
    }
}
