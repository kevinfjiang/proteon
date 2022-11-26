# proteon.rs


Lightweight and fast loadbalancer written completely in rust. Takes advantage of Redis's PUB/SUB to add and swap out backend servers.
Full extensible, with trait/interfaces defining behavior necessary to extend the project, either connection-wise or load balancing wise. Will be experimenting with additional loadbalancing techniques
Publishing to the redis backend will automatically add the struct the load balancer. You can parse the redis published messages into a struct to be added to the server pools.

Based of `Nucleon` and `Convey` rust load balancing projects


## To run:
```
docker-compose build
docker-compose up
docker exec -it {redis number} sh
```

From there
```
redis-cli
PUBLISH backend_add/backend_remove {redis => rust struct convertable server}
```

## Intentions
I wanted to improve my Rust skills and learn further about load balancers. This is not a mantained project and was built over 2 days.

## Todos
- Adding load balancing/communication techniques
- Tests, add wrappers around all external libs
- Comments