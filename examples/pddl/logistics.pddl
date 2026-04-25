(define (domain logistics)
    (:requirements :strips :typing)
    (:types truck package city location)
    (:predicates (at ?obj - (either truck package) ?loc - location)
                 (in ?pkg - package ?truck - truck)
                 (loc-at ?loc - location ?city - city))

    (:action drive-truck
        :parameters (?truck - truck ?loc-from ?loc-to - location ?city - city)
        :precondition (and (at ?truck ?loc-from) (loc-at ?loc-from ?city) (loc-at ?loc-to ?city))
        :effect (and (at ?truck ?loc-to) (not (at ?truck ?loc-from))))

    (:action fly-airplane
        :parameters (?plane - truck ?loc-from ?loc-to - location)
        :precondition (at ?plane ?loc-from)
        :effect (and (at ?plane ?loc-to) (not (at ?plane ?loc-from))))

    (:action load-truck
        :parameters (?pkg - package ?truck - truck ?loc - location)
        :precondition (and (at ?pkg ?loc) (at ?truck ?loc))
        :effect (and (in ?pkg ?truck) (not (at ?pkg ?loc))))

    (:action unload-truck
        :parameters (?pkg - package ?truck - truck ?loc - location)
        :precondition (and (in ?pkg ?truck) (at ?truck ?loc))
        :effect (and (at ?pkg ?loc) (not (in ?pkg ?truck))))
)

(define (problem logistics-simple)
    (:domain logistics)
    (:objects loc-1 loc-2 - location
              city1 - city
              truck1 - truck
              pkg1 pkg2 - package)
    (:init (at truck1 loc-1)
           (at pkg1 loc-1) (at pkg2 loc-1)
           (loc-at loc-1 city1) (loc-at loc-2 city1))
    (:goal (and (at pkg1 loc-2) (at pkg2 loc-2)))
)
