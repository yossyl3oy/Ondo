using System.Text.Json;
using System.Text.Json.Serialization;
using LibreHardwareMonitor.Hardware;

namespace OndoHardwareMonitor;

class Program
{
    static Computer? _computer;
    static UpdateVisitor? _visitor;

    static void Main(string[] args)
    {
        // Debug mode: --debug (list all sensors)
        if (args.Length > 0 && args[0] == "--debug")
        {
            RunDebug();
        }
        // Daemon mode: --daemon [interval_ms]
        else if (args.Length > 0 && args[0] == "--daemon")
        {
            int intervalMs = 1000;
            if (args.Length > 1 && int.TryParse(args[1], out int parsed))
                intervalMs = Math.Max(100, parsed);

            RunDaemon(intervalMs);
        }
        else
        {
            // Single-shot mode (backward compatible)
            RunOnce();
        }
    }

    static void RunDebug()
    {
        Computer? computer = null;
        try
        {
            computer = CreateComputer();
            computer.Open();
            computer.Accept(new UpdateVisitor());

            Console.WriteLine("=== Hardware Debug Info ===\n");

            foreach (var hardware in computer.Hardware)
            {
                Console.WriteLine($"Hardware: {hardware.Name} ({hardware.HardwareType})");

                foreach (var sensor in hardware.Sensors)
                {
                    Console.WriteLine($"  Sensor: {sensor.Name} ({sensor.SensorType}) = {sensor.Value}");
                }

                foreach (var subHardware in hardware.SubHardware)
                {
                    Console.WriteLine($"  SubHardware: {subHardware.Name} ({subHardware.HardwareType})");
                    subHardware.Update();

                    foreach (var sensor in subHardware.Sensors)
                    {
                        Console.WriteLine($"    Sensor: {sensor.Name} ({sensor.SensorType}) = {sensor.Value}");
                    }
                }
                Console.WriteLine();
            }
        }
        catch (Exception ex)
        {
            Console.WriteLine($"Error: {ex.Message}");
            Environment.Exit(1);
        }
        finally
        {
            computer?.Close();
        }
    }

    static void RunOnce()
    {
        Computer? computer = null;
        try
        {
            computer = CreateComputer();
            computer.Open();
            computer.Accept(new UpdateVisitor());

            var data = ExtractData(computer);
            OutputJson(data);
        }
        catch (Exception ex)
        {
            OutputError(ex.Message);
            Environment.Exit(1);
        }
        finally
        {
            computer?.Close();
        }
    }

    static void RunDaemon(int intervalMs)
    {
        try
        {
            _computer = CreateComputer();
            _computer.Open();
            _visitor = new UpdateVisitor();

            // Handle graceful shutdown
            Console.CancelKeyPress += (_, e) =>
            {
                e.Cancel = true;
                _computer?.Close();
                Environment.Exit(0);
            };

            // Also handle stdin close (parent process terminated)
            _ = Task.Run(() =>
            {
                try
                {
                    while (Console.Read() != -1) { }
                }
                catch { }
                _computer?.Close();
                Environment.Exit(0);
            });

            while (true)
            {
                try
                {
                    // Explicitly update all hardware and sub-hardware
                    foreach (var hardware in _computer.Hardware)
                    {
                        hardware.Update();
                        foreach (var subHardware in hardware.SubHardware)
                        {
                            subHardware.Update();
                        }
                    }

                    var data = ExtractData(_computer);
                    OutputJson(data);
                }
                catch (Exception ex)
                {
                    OutputError(ex.Message);
                }

                Thread.Sleep(intervalMs);
            }
        }
        catch (Exception ex)
        {
            OutputError(ex.Message);
            Environment.Exit(1);
        }
    }

    static Computer CreateComputer()
    {
        return new Computer
        {
            IsCpuEnabled = true,
            IsGpuEnabled = true,
            IsMemoryEnabled = true,
            IsMotherboardEnabled = true,
            IsStorageEnabled = true,
            IsControllerEnabled = true
        };
    }

    static void OutputJson(HardwareData data)
    {
        var options = new JsonSerializerOptions
        {
            WriteIndented = false,
            PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
            DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull
        };
        Console.WriteLine(JsonSerializer.Serialize(data, options));
    }

    static void OutputError(string message)
    {
        var error = new { error = message };
        var options = new JsonSerializerOptions
        {
            WriteIndented = false,
            PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower
        };
        Console.WriteLine(JsonSerializer.Serialize(error, options));
    }

    static HardwareData ExtractData(Computer computer)
    {
        var data = new HardwareData();

        foreach (var hardware in computer.Hardware)
        {
            switch (hardware.HardwareType)
            {
                case HardwareType.Cpu:
                    data.Cpu = ExtractCpuData(hardware);
                    break;
                case HardwareType.GpuNvidia:
                case HardwareType.GpuAmd:
                case HardwareType.GpuIntel:
                    if (data.Gpu == null) // Only take the first discrete GPU
                        data.Gpu = ExtractGpuData(hardware);
                    break;
                case HardwareType.Motherboard:
                    data.Motherboard = ExtractMotherboardData(hardware);
                    break;
                case HardwareType.Storage:
                    data.Storage ??= new List<StorageData>();
                    var storageData = ExtractStorageData(hardware);
                    if (storageData != null)
                        data.Storage.Add(storageData);
                    break;
            }
        }

        return data;
    }

    static CpuData ExtractCpuData(IHardware hardware)
    {
        var cpu = new CpuData { Name = hardware.Name };
        var coreTemps = new Dictionary<int, float>();
        var coreLoads = new Dictionary<int, float>();
        var coreClocks = new List<float>();

        foreach (var sensor in hardware.Sensors)
        {
            if (sensor.Value == null) continue;
            var value = sensor.Value.Value;

            switch (sensor.SensorType)
            {
                case SensorType.Temperature:
                    if (sensor.Name.Contains("Core #"))
                    {
                        if (int.TryParse(sensor.Name.Replace("Core #", "").Split(' ')[0], out int coreIndex))
                            coreTemps[coreIndex] = value;
                    }
                    else if (sensor.Name.Contains("Package") || sensor.Name.Contains("CPU") || sensor.Name.Contains("CCD"))
                    {
                        // Use the highest package/CPU temp
                        if (value > cpu.Temperature)
                            cpu.Temperature = value;
                    }
                    break;
                case SensorType.Load:
                    if (sensor.Name == "CPU Total")
                    {
                        cpu.Load = value;
                    }
                    else if (sensor.Name.Contains("Core #"))
                    {
                        if (int.TryParse(sensor.Name.Replace("CPU Core #", "").Split(' ')[0], out int coreIndex))
                            coreLoads[coreIndex] = value;
                    }
                    break;
                case SensorType.Clock:
                    // Collect all core clocks for averaging
                    if (sensor.Name.Contains("Core #"))
                    {
                        coreClocks.Add(value);
                    }
                    break;
            }
        }

        // Calculate average frequency from all cores
        if (coreClocks.Count > 0)
        {
            cpu.Frequency = coreClocks.Average() / 1000f; // MHz to GHz
        }

        // Build core data
        var maxCores = Math.Max(
            coreTemps.Count > 0 ? coreTemps.Keys.Max() + 1 : 0,
            coreLoads.Count > 0 ? coreLoads.Keys.Max() + 1 : 0
        );

        if (maxCores > 0)
        {
            cpu.Cores = new List<CpuCoreData>();
            for (int i = 0; i < maxCores; i++)
            {
                cpu.Cores.Add(new CpuCoreData
                {
                    Index = (uint)i,
                    Temperature = coreTemps.GetValueOrDefault(i, cpu.Temperature),
                    Load = coreLoads.GetValueOrDefault(i, cpu.Load)
                });
            }
        }

        cpu.MaxTemperature = 100f;
        return cpu;
    }

    static GpuData ExtractGpuData(IHardware hardware)
    {
        var gpu = new GpuData { Name = hardware.Name };

        foreach (var sensor in hardware.Sensors)
        {
            if (sensor.Value == null) continue;
            var value = sensor.Value.Value;

            switch (sensor.SensorType)
            {
                case SensorType.Temperature:
                    if (sensor.Name.Contains("GPU Core") || sensor.Name == "GPU" || sensor.Name == "Temperature")
                        gpu.Temperature = value;
                    break;
                case SensorType.Load:
                    if (sensor.Name == "GPU Core" || sensor.Name == "GPU")
                        gpu.Load = value;
                    break;
                case SensorType.Clock:
                    if (sensor.Name == "GPU Core" || sensor.Name.Contains("GPU Core"))
                        gpu.Frequency = value / 1000f; // MHz to GHz
                    break;
                case SensorType.SmallData:
                    if (sensor.Name == "GPU Memory Used" || sensor.Name == "D3D Dedicated Memory Used")
                        gpu.MemoryUsed = value / 1024f; // MB to GB
                    else if (sensor.Name == "GPU Memory Total")
                        gpu.MemoryTotal = value / 1024f; // MB to GB
                    break;
            }
        }

        gpu.MaxTemperature = 95f;
        return gpu;
    }

    static MotherboardData ExtractMotherboardData(IHardware hardware)
    {
        var mb = new MotherboardData { Name = hardware.Name, Fans = new List<FanData>() };
        var temps = new List<float>();

        // Helper to process sensors from any hardware
        void ProcessSensors(IHardware hw)
        {
            foreach (var sensor in hw.Sensors)
            {
                if (sensor.Value == null) continue;
                var value = sensor.Value.Value;

                switch (sensor.SensorType)
                {
                    case SensorType.Temperature:
                        // Collect motherboard-related temperatures (exclude CPU temps)
                        if (!sensor.Name.Contains("CPU") && !sensor.Name.Contains("Core") && value > 0 && value < 150)
                        {
                            temps.Add(value);
                            // Prefer specific motherboard temps (expanded list)
                            var lowerName = sensor.Name.ToLower();
                            if (lowerName.Contains("system") || lowerName.Contains("motherboard") ||
                                lowerName.Contains("mainboard") || lowerName.Contains("pch") ||
                                lowerName.Contains("vrm") || lowerName.Contains("chipset") ||
                                lowerName.Contains("mos") || lowerName.Contains("vsoc") ||
                                lowerName.Contains("auxtin") || lowerName.Contains("temp1") ||
                                sensor.Name.StartsWith("Temperature #"))
                            {
                                // Use the first matching temp if not already set
                                if (mb.Temperature == 0)
                                {
                                    mb.Temperature = value;
                                }
                            }
                        }
                        break;
                    case SensorType.Fan:
                        if (value > 0)
                        {
                            mb.Fans.Add(new FanData
                            {
                                Name = sensor.Name,
                                Speed = (uint)value
                            });
                        }
                        break;
                }
            }
        }

        // Check main hardware sensors first
        ProcessSensors(hardware);

        // Check sub-hardware for sensors (motherboard sensors are usually in sub-hardware like SuperIO chips)
        foreach (var subHardware in hardware.SubHardware)
        {
            subHardware.Update();
            ProcessSensors(subHardware);

            // Also check nested sub-hardware
            foreach (var nestedSub in subHardware.SubHardware)
            {
                nestedSub.Update();
                ProcessSensors(nestedSub);
            }
        }

        // If no specific temp found, use first available temp or average
        if (mb.Temperature == 0 && temps.Count > 0)
        {
            // Use the first temperature as it's usually the most relevant
            mb.Temperature = temps[0];
        }

        return mb;
    }

    static StorageData? ExtractStorageData(IHardware hardware)
    {
        var storage = new StorageData { Name = hardware.Name };

        // Helper to extract sensors from hardware
        void ExtractSensors(IHardware hw)
        {
            foreach (var sensor in hw.Sensors)
            {
                if (sensor.Value == null) continue;
                var value = sensor.Value.Value;

                switch (sensor.SensorType)
                {
                    case SensorType.Temperature:
                        // Take any temperature sensor (NVMe drives may have multiple)
                        if (value > 0 && value < 100 && storage.Temperature == 0)
                        {
                            storage.Temperature = value;
                        }
                        break;
                    case SensorType.Load:
                        if (sensor.Name == "Used Space" || sensor.Name.Contains("Used"))
                        {
                            storage.UsedPercent = value;
                        }
                        break;
                    case SensorType.Level:
                        // Some drives report temperature as Level (S.M.A.R.T. attribute 194)
                        var lowerName = sensor.Name.ToLower();
                        if ((lowerName.Contains("temperature") || lowerName.Contains("temp")) &&
                            value > 0 && value < 100 && storage.Temperature == 0)
                        {
                            storage.Temperature = value;
                        }
                        break;
                    case SensorType.Data:
                        // Total Bytes Written/Read - can use to estimate drive health
                        break;
                }
            }
        }

        // Extract from main hardware
        ExtractSensors(hardware);

        // Also check sub-hardware (some drives have sensors in sub-hardware)
        foreach (var subHardware in hardware.SubHardware)
        {
            subHardware.Update();
            ExtractSensors(subHardware);
        }

        // Try to get drive capacity from hardware name (e.g., "Samsung SSD 980 PRO 1TB", "WD Blue 500GB")
        var name = hardware.Name;
        if (name.Contains("TB"))
        {
            var tbMatch = System.Text.RegularExpressions.Regex.Match(name, @"(\d+(?:\.\d+)?)\s*TB", System.Text.RegularExpressions.RegexOptions.IgnoreCase);
            if (tbMatch.Success && float.TryParse(tbMatch.Groups[1].Value, out float tb))
            {
                storage.TotalSpace = tb * 1024; // Convert TB to GB
            }
        }
        else if (name.Contains("GB"))
        {
            var gbMatch = System.Text.RegularExpressions.Regex.Match(name, @"(\d+)\s*GB", System.Text.RegularExpressions.RegexOptions.IgnoreCase);
            if (gbMatch.Success && float.TryParse(gbMatch.Groups[1].Value, out float gb))
            {
                storage.TotalSpace = gb;
            }
        }

        // Return storage data even if we only have the name (temperature/usage may be unavailable)
        return storage;
    }
}

class UpdateVisitor : IVisitor
{
    public void VisitComputer(IComputer computer)
    {
        computer.Traverse(this);
    }

    public void VisitHardware(IHardware hardware)
    {
        hardware.Update();
        foreach (var subHardware in hardware.SubHardware)
            subHardware.Accept(this);
    }

    public void VisitSensor(ISensor sensor) { }
    public void VisitParameter(IParameter parameter) { }
}

// Data models matching Rust structs
class HardwareData
{
    public CpuData? Cpu { get; set; }
    public GpuData? Gpu { get; set; }
    public List<StorageData>? Storage { get; set; }
    public MotherboardData? Motherboard { get; set; }
}

class CpuData
{
    public string Name { get; set; } = "";
    public float Temperature { get; set; }
    public float MaxTemperature { get; set; }
    public float Load { get; set; }
    public float Frequency { get; set; }
    public List<CpuCoreData>? Cores { get; set; }
}

class CpuCoreData
{
    public uint Index { get; set; }
    public float Temperature { get; set; }
    public float Load { get; set; }
}

class GpuData
{
    public string Name { get; set; } = "";
    public float Temperature { get; set; }
    public float MaxTemperature { get; set; }
    public float Load { get; set; }
    public float Frequency { get; set; }
    public float MemoryUsed { get; set; }
    public float MemoryTotal { get; set; }
}

class StorageData
{
    public string Name { get; set; } = "";
    public float Temperature { get; set; }
    public float UsedPercent { get; set; }
    public float TotalSpace { get; set; }
}

class MotherboardData
{
    public string Name { get; set; } = "";
    public float Temperature { get; set; }
    public List<FanData>? Fans { get; set; }
}

class FanData
{
    public string Name { get; set; } = "";
    public uint Speed { get; set; }
}
