# NetRTS(Flaud)

A bot for a simplified version of StarCraft (NetRTS) for the SnowCollege coding competition 2026-2027.

There are two Three bot implementations one is an algorithmically based bod, the second is managed by the open-source fly brain, and the final bot is a test bot based on the provided example bots.

- Flaud -- Fly Brain
- RustalaDaora -- Algorithm-Based

## Running

In order to use this to run the seperate bots you would need to add the api keys to a .env file under the specified names, eg:

``` .env
Flaud="TOTALLY_REAL_API_KEY"
```

You then pass the name of the bot under the named argument `--name Flaud` to fetch that variable. The code will then attempt to matchmake you and run your bot logic.

### NOTE

---

To get all possible arguments and what they do run:

``` shell
cargo --help
```

(or)

``` shell
NetRts_Flaud_.exe --help
```
