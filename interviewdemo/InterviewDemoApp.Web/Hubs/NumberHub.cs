using InterviewDemoApp.Core;
using Microsoft.AspNetCore.SignalR;

namespace InterviewDemoApp.Web.Hubs;

public class NumberHub: Hub<INumberClientHub>
{
    private readonly INumberManager _numberManager;
    public NumberHub(INumberManager numberManager)
    {
        _numberManager = numberManager;
    }
    private bool _setupWasCalled = false;
    public async Task SetupIntervals(int interval)
    {
        await Clients.Caller.SendMessage($"Setup intervals for {interval} seconds.");
        _setupWasCalled = true;

        _numberManager.OnTick += async (_, list) =>
        {
            await Clients.Caller.SendMessage(KeyPairListToString(list));
        };
    }

    private static string KeyPairListToString(IEnumerable<KeyValuePair<ulong, ulong>> list)
    {
       return string.Join(",", list.Select(p => $"{p.Key}:{p.Value}"));
    }

    public async Task AddNumber(ulong number)
    {
        if (!_setupWasCalled)
        {
            await Clients.Caller.SendMessage("Error - You need to call SetupIntervals first!");
        }

        _numberManager.AddNumber(number);
    }

    public async Task Halt()
    {
        if (!_setupWasCalled)
        {
            await Clients.Caller.SendMessage("Error - You need to call SetupIntervals first!");
            return;
        }
        
        _numberManager.Halt();
    }

    public async Task Resume()
    {
        if (!_setupWasCalled)
        {
            await Clients.Caller.SendMessage("Error - You need to call SetupIntervals first!");
            return;
        }
        
        _numberManager.Resume();
    }

    public async Task Quit()
    {
        if (!_setupWasCalled)
        {
            await Clients.Caller.SendMessage("Error - You need to call SetupIntervals first!");
            return;
        }

        await Clients.Caller.SendMessage(KeyPairListToString(_numberManager.GetCounts()));
        Context.Abort();
    }
}